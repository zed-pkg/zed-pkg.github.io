use std::collections::{BTreeMap, BTreeSet};

pub type MappingFn = fn() -> BTreeMap<&'static str, &'static str>;

#[derive(Clone, Copy)]
pub struct ServiceContract {
    pub service: &'static str,
    pub title: &'static str,
    pub private_listener: bool,
    pub mutable_keys: &'static [&'static str],
    pub secret_keys: &'static [&'static str],
    pub flag_to_env: MappingFn,
    pub env_to_config: MappingFn,
}

impl ServiceContract {
    pub fn validate(self) -> Result<(), String> {
        if self.service.is_empty()
            || self.title.is_empty()
            || self.service.len() > 128
            || !self
                .service
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err("service identity is invalid".to_owned());
        }

        let mutable: BTreeSet<_> = self.mutable_keys.iter().copied().collect();
        let secrets: BTreeSet<_> = self.secret_keys.iter().copied().collect();
        if let Some(key) = mutable.intersection(&secrets).next() {
            return Err(format!("runtime mutable key {key} is classified as secret"));
        }

        let env_to_config = (self.env_to_config)();
        if let Some(key) = mutable
            .iter()
            .find(|key| !env_to_config.contains_key(**key))
        {
            return Err(format!("runtime mutable key {key} lacks a config mapping"));
        }

        let flag_to_env = (self.flag_to_env)();
        if flag_to_env
            .values()
            .any(|env_name| secrets.contains(env_name))
        {
            return Err("a secret environment variable is exposed as a CLI flag".to_owned());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([("maintenance-mode", "MAINTENANCE_MODE")])
    }

    fn config() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([("MAINTENANCE_MODE", "runtime.maintenance")])
    }

    #[test]
    fn accepts_disjoint_mapped_contract() {
        let contract = ServiceContract {
            service: "example-api-server",
            title: "Example API",
            private_listener: false,
            mutable_keys: &["MAINTENANCE_MODE"],
            secret_keys: &["REDIS_URL"],
            flag_to_env: flags,
            env_to_config: config,
        };
        assert!(contract.validate().is_ok());
    }
}
