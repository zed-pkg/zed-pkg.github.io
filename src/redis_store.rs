use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use futures_util::StreamExt;
use redis::{aio::ConnectionManager, Script};
use tokio::sync::Mutex;

use crate::{
    protocol::unix_timestamp_string, CacheEvent, CacheOperation, CacheSnapshot, Error, EventStream,
    Keyspace, Mutation, Result, SnapshotStore, MAX_EVENT_BYTES, MAX_SAFE_REVISION,
    MAX_SNAPSHOT_BYTES,
};

const MUTATE_LUA: &str = r#"
local op = ARGV[1]
local source = ARGV[2]
local published_at = ARGV[3]
local count_text = ARGV[4]

local MAX_SAFE_REVISION_BEFORE_INCREMENT = 9007199254740990
local MAX_ITEMS = 1024
local MAX_KEY_BYTES = 512
local MAX_VALUE_BYTES = 1048576
local MAX_MUTATION_BYTES = 4194304
local MAX_EVENT_BYTES = 4194304
local MAX_SEGMENT_BYTES = 64
local MAX_SOURCE_BYTES = 256
local MAX_TIMESTAMP_BYTES = 64

local function fail(message)
  return redis.error_reply('ores.lru-redis.v1: ' .. message)
end

local function valid_segment(value)
  return value ~= nil
    and string.len(value) > 0
    and string.len(value) <= MAX_SEGMENT_BYTES
    and string.match(value, '^[A-Za-z0-9._-]+$') ~= nil
end

local function valid_text(value, maximum)
  return value ~= nil
    and value ~= ''
    and string.len(value) <= maximum
    and string.find(value, '%c') == nil
end

local function valid_cache_key(value)
  return valid_text(value, MAX_KEY_BYTES)
end

local function matches_keyspace(snapshot_suffix, meta_suffix, events_suffix, namespace, cache)
  local identity = namespace .. ':' .. cache
  local cluster_prefix = 'ores:lru:v1:{' .. identity .. '}'
  local legacy_prefix = 'ores:lru:v1:' .. identity
  local function matches(prefix)
    return KEYS[1] == prefix .. snapshot_suffix
      and KEYS[2] == prefix .. meta_suffix
      and KEYS[3] == prefix .. events_suffix
  end
  return matches(cluster_prefix) or matches(legacy_prefix)
end

if #KEYS ~= 3 then
  return fail('expected exactly 3 keys')
end

if op ~= 'upsert' and op ~= 'delete' and op ~= 'replace' and op ~= 'invalidate' and op ~= 'resync' then
  return fail('unsupported operation')
end

if not valid_text(source, MAX_SOURCE_BYTES) then
  return fail('source is invalid')
end

if not valid_text(published_at, MAX_TIMESTAMP_BYTES) then
  return fail('published_at is invalid')
end

if count_text == nil or string.match(count_text, '^%d+$') == nil then
  return fail('count must be a non-negative integer')
end

local count = tonumber(count_text)
if count == nil or count ~= math.floor(count) or count > MAX_ITEMS then
  return fail('count is out of range')
end

local payload_size = 0
if op == 'upsert' or op == 'replace' then
  payload_size = count * 2
  if op == 'upsert' and count == 0 then
    return fail('upsert requires at least one entry')
  end
elseif op == 'delete' then
  payload_size = count
  if count == 0 then
    return fail('delete requires at least one key')
  end
elseif count ~= 0 then
  return fail(op .. ' requires count 0')
end

if #ARGV ~= 6 + payload_size then
  return fail('argument count does not match payload')
end

local namespace = ARGV[5 + payload_size]
local cache = ARGV[6 + payload_size]
if not valid_segment(namespace) or not valid_segment(cache) then
  return fail('invalid namespace or cache')
end

if not matches_keyspace(':snapshot', ':meta', ':events', namespace, cache) then
  return fail('keyspace mismatch')
end

-- Force wrong-type and revision failures before the first write. Redis scripts provide
-- isolation, not rollback after a later runtime error.
local snapshot_size = redis.call('HLEN', KEYS[1])
local current_revision = redis.call('HGET', KEYS[2], 'revision')
local current_revision_number = 0
if current_revision ~= false then
  if current_revision ~= '0' and string.match(current_revision, '^[1-9]%d*$') == nil then
    return fail('revision must be a canonical non-negative integer')
  end
  current_revision_number = tonumber(current_revision)
  if current_revision_number == nil
    or current_revision_number ~= math.floor(current_revision_number)
    or current_revision_number > MAX_SAFE_REVISION_BEFORE_INCREMENT
  then
    return fail('revision is out of range')
  end
elseif snapshot_size > 0 then
  return fail('snapshot exists without revision metadata')
end

local cursor = 5
local entries = cjson.decode('{}')
local keys = cjson.decode('[]')
local seen_keys = {}
local mutation_bytes = 0

if op == 'upsert' or op == 'replace' then
  for _ = 1, count do
    local key = ARGV[cursor]
    local value = ARGV[cursor + 1]
    if not valid_cache_key(key) then
      return fail('entry key is invalid')
    end
    if value == nil or string.len(value) > MAX_VALUE_BYTES then
      return fail('entry value is invalid')
    end
    if seen_keys[key] then
      return fail('entry keys must be unique')
    end
    seen_keys[key] = true
    mutation_bytes = mutation_bytes + string.len(key) + string.len(value)
    if mutation_bytes > MAX_MUTATION_BYTES then
      return fail('mutation payload is too large')
    end
    cursor = cursor + 2
    entries[key] = value
  end
elseif op == 'delete' then
  for _ = 1, count do
    local key = ARGV[cursor]
    if not valid_cache_key(key) then
      return fail('delete key is invalid')
    end
    if seen_keys[key] then
      return fail('delete keys must be unique')
    end
    seen_keys[key] = true
    mutation_bytes = mutation_bytes + string.len(key)
    if mutation_bytes > MAX_MUTATION_BYTES then
      return fail('mutation payload is too large')
    end
    cursor = cursor + 1
    table.insert(keys, key)
  end
end

local revision = current_revision_number + 1
local revision_text = string.format('%.0f', revision)
local event_payload = {
  protocol = 'ores.lru-redis.v1',
  namespace = namespace,
  cache = cache,
  operation = op,
  source = source,
  published_at = published_at
}

if op == 'upsert' or op == 'replace' then
  event_payload.entries = entries
elseif op == 'delete' then
  event_payload.keys = keys
end

-- Lua CJSON supports at most 14 significant digits, less than the 16 digits needed for
-- 2^53-1. Let CJSON escape every string/object field, then append the validated decimal
-- revision token without routing it through CJSON's lossy number formatter.
local event_without_revision = cjson.encode(event_payload)
local event = string.sub(event_without_revision, 1, -2)
  .. ',"revision":' .. revision_text .. '}'
if string.len(event) > MAX_EVENT_BYTES then
  return fail('event payload is too large')
end

if op == 'replace' or op == 'invalidate' then
  redis.call('DEL', KEYS[1])
end

if op == 'upsert' or op == 'replace' then
  for key, value in pairs(entries) do
    redis.call('HSET', KEYS[1], key, value)
  end
elseif op == 'delete' then
  for _, key in ipairs(keys) do
    redis.call('HDEL', KEYS[1], key)
  end
end

redis.call('HSET', KEYS[2], 'revision', revision_text, 'updated_at', published_at)
redis.call('PUBLISH', KEYS[3], event)
return event
"#;

const READ_SNAPSHOT_LUA: &str = r"
local MAX_SAFE_REVISION = 9007199254740991
local MAX_ITEMS = 100000
local MAX_KEY_BYTES = 512
local MAX_VALUE_BYTES = 1048576
local MAX_SNAPSHOT_BYTES = 67108864
local MAX_SEGMENT_BYTES = 64

local function fail(message)
  return redis.error_reply('ores.lru-redis.v1: ' .. message)
end

local function valid_segment(value)
  return value ~= nil
    and string.len(value) > 0
    and string.len(value) <= MAX_SEGMENT_BYTES
    and string.match(value, '^[A-Za-z0-9._-]+$') ~= nil
end

local function valid_cache_key(value)
  return value ~= nil
    and value ~= ''
    and string.len(value) <= MAX_KEY_BYTES
    and string.find(value, '%c') == nil
end

if #KEYS ~= 2 or #ARGV ~= 2 then
  return fail('snapshot read expects exactly 2 keys and 2 arguments')
end

local namespace = ARGV[1]
local cache = ARGV[2]
if not valid_segment(namespace) or not valid_segment(cache) then
  return fail('invalid namespace or cache')
end

local identity = namespace .. ':' .. cache
local cluster_prefix = 'ores:lru:v1:{' .. identity .. '}'
local legacy_prefix = 'ores:lru:v1:' .. identity
local function matches(prefix)
  return KEYS[1] == prefix .. ':snapshot' and KEYS[2] == prefix .. ':meta'
end
if not matches(cluster_prefix) and not matches(legacy_prefix) then
  return fail('keyspace mismatch')
end

local snapshot_size = redis.call('HLEN', KEYS[1])
if snapshot_size > MAX_ITEMS then
  return fail('snapshot entry count is too large')
end

local revision = redis.call('HGET', KEYS[2], 'revision')
if revision == false then
  if snapshot_size > 0 then
    return fail('snapshot exists without revision metadata')
  end
  revision = '0'
elseif revision ~= '0' and string.match(revision, '^[1-9]%d*$') == nil then
  return fail('revision must be a canonical non-negative integer')
end

local revision_number = tonumber(revision)
if revision_number == nil
  or revision_number ~= math.floor(revision_number)
  or revision_number > MAX_SAFE_REVISION
then
  return fail('revision is out of range')
end

local flat_entries = redis.call('HGETALL', KEYS[1])
local snapshot_bytes = 0
for index = 1, #flat_entries, 2 do
  local key = flat_entries[index]
  local value = flat_entries[index + 1]
  if not valid_cache_key(key) then
    return fail('snapshot contains an invalid key')
  end
  if value == nil or string.len(value) > MAX_VALUE_BYTES then
    return fail('snapshot contains an oversized value')
  end
  snapshot_bytes = snapshot_bytes + string.len(key) + string.len(value)
  if snapshot_bytes > MAX_SNAPSHOT_BYTES then
    return fail('snapshot payload is too large')
  end
end

local response = { revision }
for index = 1, #flat_entries do
  table.insert(response, flat_entries[index])
end
return response
";

pub struct RedisStore {
    client: redis::Client,
    commands: Arc<Mutex<ConnectionManager>>,
}

impl RedisStore {
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let commands = ConnectionManager::new(client.clone()).await?;
        Ok(Self {
            client,
            commands: Arc::new(Mutex::new(commands)),
        })
    }
}

#[async_trait]
impl SnapshotStore for RedisStore {
    async fn read_snapshot(&self, keyspace: &Keyspace) -> Result<CacheSnapshot> {
        let script = Script::new(READ_SNAPSHOT_LUA);
        let mut invocation = script.prepare_invoke();
        invocation
            .key(keyspace.snapshot_key())
            .key(keyspace.meta_key())
            .arg(keyspace.namespace())
            .arg(keyspace.cache());

        let values: Vec<String> = {
            let mut connection = self.commands.lock().await;
            invocation.invoke_async(&mut *connection).await?
        };
        let (revision_text, flat_entries) = values
            .split_first()
            .ok_or(Error::InvalidEvent("snapshot script returned no revision"))?;
        if flat_entries.len() % 2 != 0 {
            return Err(Error::InvalidEvent(
                "snapshot script returned an odd entry payload",
            ));
        }
        let revision = parse_revision(Some(revision_text))?;
        let mut entries = BTreeMap::new();
        let mut payload_bytes = 0usize;
        for pair in flat_entries.chunks_exact(2) {
            payload_bytes = payload_bytes
                .saturating_add(pair[0].len())
                .saturating_add(pair[1].len());
            if payload_bytes > MAX_SNAPSHOT_BYTES {
                return Err(Error::PayloadLimitExceeded {
                    kind: "snapshot payload bytes",
                    actual: payload_bytes,
                    max: MAX_SNAPSHOT_BYTES,
                });
            }
            entries.insert(pair[0].clone(), pair[1].clone());
        }
        let snapshot = CacheSnapshot { revision, entries };
        snapshot.validate_bounds()?;
        Ok(snapshot)
    }

    async fn mutate(
        &self,
        keyspace: &Keyspace,
        mutation: Mutation,
        source: &str,
    ) -> Result<CacheEvent> {
        let (operation, entries, keys) = match mutation {
            Mutation::Upsert(entries) => (CacheOperation::Upsert, entries, Vec::new()),
            Mutation::Delete(keys) => (CacheOperation::Delete, BTreeMap::new(), keys),
            Mutation::Replace(entries) => (CacheOperation::Replace, entries, Vec::new()),
            Mutation::Invalidate => (CacheOperation::Invalidate, BTreeMap::new(), Vec::new()),
            Mutation::Resync => (CacheOperation::Resync, BTreeMap::new(), Vec::new()),
        };

        let published_at = unix_timestamp_string();
        let mut validation = CacheEvent {
            protocol: crate::PROTOCOL.to_owned(),
            namespace: keyspace.namespace().to_owned(),
            cache: keyspace.cache().to_owned(),
            revision: 1,
            operation,
            entries: entries.clone(),
            keys: keys.clone(),
            source: source.to_owned(),
            published_at: published_at.clone(),
        };
        validation.validate()?;
        validation.entries.clear();
        validation.keys.clear();

        let operation_name = match operation {
            CacheOperation::Upsert => "upsert",
            CacheOperation::Delete => "delete",
            CacheOperation::Replace => "replace",
            CacheOperation::Invalidate => "invalidate",
            CacheOperation::Resync => "resync",
        };

        let script = Script::new(MUTATE_LUA);
        let mut invocation = script.prepare_invoke();
        invocation
            .key(keyspace.snapshot_key())
            .key(keyspace.meta_key())
            .key(keyspace.event_channel())
            .arg(operation_name)
            .arg(source)
            .arg(published_at);

        if entries.is_empty() {
            invocation.arg(keys.len());
            for key in keys {
                invocation.arg(key);
            }
        } else {
            invocation.arg(entries.len());
            for (key, value) in entries {
                invocation.arg(key).arg(value);
            }
        }
        invocation.arg(keyspace.namespace()).arg(keyspace.cache());

        let payload: String = {
            let mut connection = self.commands.lock().await;
            invocation.invoke_async(&mut *connection).await?
        };
        if payload.len() > MAX_EVENT_BYTES {
            return Err(Error::PayloadLimitExceeded {
                kind: "event bytes",
                actual: payload.len(),
                max: MAX_EVENT_BYTES,
            });
        }
        let event: CacheEvent = serde_json::from_str(&payload)?;
        event.validate()?;
        Ok(event)
    }

    async fn subscribe(&self, keyspace: &Keyspace) -> Result<EventStream> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        pubsub.subscribe(keyspace.event_channel()).await?;
        let stream = pubsub.into_on_message().map(|message| {
            let payload: Vec<u8> = message.get_payload_bytes().to_vec();
            if payload.len() > MAX_EVENT_BYTES {
                return Err(Error::PayloadLimitExceeded {
                    kind: "event bytes",
                    actual: payload.len(),
                    max: MAX_EVENT_BYTES,
                });
            }
            let event: CacheEvent = serde_json::from_slice(&payload)?;
            event.validate()?;
            Ok(event)
        });
        Ok(Box::pin(stream))
    }
}

fn parse_revision(value: Option<&str>) -> Result<u64> {
    let Some(value) = value else {
        return Ok(0);
    };
    let revision = value
        .parse::<u64>()
        .map_err(|_| Error::InvalidEvent("Redis revision metadata is not an integer"))?;
    if value != revision.to_string() {
        return Err(Error::InvalidEvent(
            "Redis revision metadata is not canonical",
        ));
    }
    if revision > MAX_SAFE_REVISION {
        return Err(Error::RevisionOutOfRange {
            revision,
            max: MAX_SAFE_REVISION,
        });
    }
    Ok(revision)
}

#[cfg(test)]
mod tests {
    use super::{parse_revision, MUTATE_LUA, READ_SNAPSHOT_LUA};
    use crate::{Error, MAX_SAFE_REVISION};

    #[test]
    fn mutation_script_finishes_deterministic_work_before_first_write() {
        let arity_check = MUTATE_LUA
            .find("if #ARGV ~= 6 + payload_size")
            .expect("arity guard must exist");
        let type_check = MUTATE_LUA
            .find("redis.call('HLEN', KEYS[1])")
            .expect("snapshot type guard must exist");
        let event_encode = MUTATE_LUA
            .find("local event_without_revision = cjson.encode(event_payload)")
            .expect("event encoding must exist");
        let first_write = MUTATE_LUA
            .find("redis.call('DEL', KEYS[1])")
            .expect("mutation must exist");

        assert!(arity_check < first_write);
        assert!(type_check < first_write);
        assert!(event_encode < first_write);
        assert!(MUTATE_LUA.contains("local cluster_prefix = 'ores:lru:v1:{'"));
        assert!(MUTATE_LUA.contains("local legacy_prefix = 'ores:lru:v1:'"));
        assert!(MUTATE_LUA.contains("MAX_SAFE_REVISION_BEFORE_INCREMENT = 9007199254740990"));
        assert!(MUTATE_LUA.contains("local revision_text = string.format('%.0f', revision)"));
        assert!(MUTATE_LUA.contains(".. ',\"revision\":' .. revision_text .. '}'"));
        assert!(!MUTATE_LUA.contains("encode_number_precision"));
        assert!(MUTATE_LUA.contains("return fail('mutation payload is too large')"));
        assert!(MUTATE_LUA.contains("return fail('event payload is too large')"));
    }

    #[test]
    fn snapshot_script_reads_revision_and_entries_in_one_atomic_execution() {
        assert!(READ_SNAPSHOT_LUA.contains("redis.call('HLEN', KEYS[1])"));
        assert!(READ_SNAPSHOT_LUA.contains("redis.call('HGET', KEYS[2], 'revision')"));
        assert!(READ_SNAPSHOT_LUA.contains("redis.call('HGETALL', KEYS[1])"));
        assert!(READ_SNAPSHOT_LUA.contains("MAX_SAFE_REVISION = 9007199254740991"));
        assert!(READ_SNAPSHOT_LUA.contains("local cluster_prefix = 'ores:lru:v1:{'"));
        assert!(READ_SNAPSHOT_LUA.contains("local legacy_prefix = 'ores:lru:v1:'"));
    }

    #[test]
    fn revision_metadata_is_canonical_and_cross_runtime_safe() {
        assert_eq!(parse_revision(None).unwrap(), 0);
        assert_eq!(parse_revision(Some("0")).unwrap(), 0);
        assert_eq!(parse_revision(Some("1")).unwrap(), 1);
        assert!(parse_revision(Some("01")).is_err());
        assert!(parse_revision(Some("-1")).is_err());
        assert!(matches!(
            parse_revision(Some(&(MAX_SAFE_REVISION + 1).to_string())),
            Err(Error::RevisionOutOfRange { .. })
        ));
    }
}
