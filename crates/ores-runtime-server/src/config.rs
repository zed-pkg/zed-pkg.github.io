use std::{
    collections::{BTreeMap, HashMap},
    io,
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use flags2env::BundledFlags2Env;
use serde::Deserialize;

use crate::{BoxError, ServiceContract, runtime_env::RuntimeEnvConfig};

const CONTRACT_PATH: &str = ".cli-flags.toml";

#[derive(Debug, Deserialize)]
struct RawConfig {
    #[serde(rename = "SERVICE_HOST")]
    host: String,
    #[serde(rename = "PORT")]
    port: u16,
    #[serde(rename = "RUST_LOG")]
    log_filter: String,
    #[serde(rename = "K_REVISION")]
    revision: String,
    #[serde(rename = "REDIS_LRU_NAMESPACE")]
    cache_namespace: String,
    #[serde(rename = "REDIS_LRU_CAPACITY")]
    cache_capacity: usize,
    #[serde(rename = "REDIS_LRU_POLL_SECONDS")]
    cache_repair_seconds: u64,
    #[serde(rename = "REDIS_LRU_REQUIRED")]
    redis_required: bool,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub bind: SocketAddr,
    pub log_filter: String,
    pub revision: String,
    pub runtime_env: RuntimeEnvConfig,
}

impl Config {
    /// Resolve process configuration through the checked `flags2env` contract.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid mapping contract, command-line input, typed
    /// value, listener address, cache bound, or missing required Redis URL.
    pub fn from_process(contract: ServiceContract) -> Result<Self, BoxError> {
        contract.validate().map_err(invalid)?;

        let parser = BundledFlags2Env::new();
        parser
            .audit_config(Some(CONTRACT_PATH))
            .map_err(|error| invalid(format!("flags2env audit failed: {error}")))?;
        let parsed = parser
            .parse_structured(&std::env::args().collect::<Vec<_>>(), Some(CONTRACT_PATH))
            .map_err(|error| invalid(format!("flags2env parsing failed: {error}")))?;
        if !parsed.unknown_options.is_empty()
            || !parsed.errors.is_empty()
            || !parsed.extras.is_empty()
        {
            return Err(invalid(format!(
                "invalid arguments: unknown={}, errors={}, extras={}",
                parsed.unknown_options.len(),
                parsed.errors.len(),
                parsed.extras.len()
            ))
            .into());
        }

        let mut values: HashMap<String, String> = parsed.dotenv;
        values.extend(std::env::vars());
        values.extend(parsed.dotenv_overrides);
        values.extend(parsed.provided_flags);
        let raw: RawConfig = parser
            .coerce(&values, Some(CONTRACT_PATH))
            .map_err(|error| invalid(format!("flags2env coercion failed: {error}")))?;

        if raw.cache_capacity == 0 {
            return Err(invalid("REDIS_LRU_CAPACITY must be greater than zero").into());
        }
        if !(1..=180).contains(&raw.cache_repair_seconds) {
            return Err(invalid("REDIS_LRU_POLL_SECONDS must be between 1 and 180").into());
        }
        let cache_namespace = raw.cache_namespace.trim().to_owned();
        if cache_namespace.is_empty() {
            return Err(invalid("REDIS_LRU_NAMESPACE must not be empty").into());
        }

        let host: IpAddr = raw
            .host
            .parse()
            .map_err(|_| invalid("SERVICE_HOST must be an IP address"))?;
        if contract.private_listener
            && !host.is_loopback()
            && !optional_bool(&values, "ADMIN_ALLOW_PUBLIC_BIND", false)?
        {
            return Err(invalid(
                "private server refuses a non-loopback bind without ADMIN_ALLOW_PUBLIC_BIND=true",
            )
            .into());
        }

        let redis_url = optional(&values, "REDIS_URL");
        if raw.redis_required && redis_url.is_none() {
            return Err(invalid("REDIS_URL is required when REDIS_LRU_REQUIRED=true").into());
        }

        let mut baseline = BTreeMap::new();
        for key in contract.mutable_keys {
            if contract.secret_keys.contains(key) {
                return Err(invalid(format!("mutable key {key} is classified as secret")).into());
            }
            if let Some(value) = optional(&values, key) {
                baseline.insert((*key).to_owned(), value);
            }
        }

        Ok(Self {
            bind: SocketAddr::new(host, raw.port),
            log_filter: raw.log_filter,
            revision: raw.revision,
            runtime_env: RuntimeEnvConfig {
                redis_url,
                redis_required: raw.redis_required,
                namespace: cache_namespace,
                capacity: raw.cache_capacity,
                repair_interval: Duration::from_secs(raw.cache_repair_seconds),
                baseline,
            },
        })
    }
}

fn optional(values: &HashMap<String, String>, name: &str) -> Option<String> {
    values
        .get(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn optional_bool(
    values: &HashMap<String, String>,
    name: &str,
    default: bool,
) -> Result<bool, BoxError> {
    let Some(value) = optional(values, name) else {
        return Ok(default);
    };
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(invalid(format!("{name} must be a boolean")).into()),
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
