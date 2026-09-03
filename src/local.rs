use std::sync::Arc;

use crate::{CachePolicy, DisabledBackend, LocalOnly, Result, RuntimeConfig, SyncRuntime};

/// Runtime alias for a local LRU with no network or external key/value dependency.
pub type LocalRuntime<P> = SyncRuntime<LocalOnly<P>, DisabledBackend>;

impl<P: CachePolicy> SyncRuntime<LocalOnly<P>, DisabledBackend> {
    /// Construct a runtime that never opens a Redis or other backend connection.
    ///
    /// The local cache still enforces the wrapped policy's capacity, key allowlist,
    /// and redaction rules. Any accidental direct I/O against `DisabledBackend`
    /// fails closed with `Error::BackendDisabled`.
    pub fn local_only(namespace: impl Into<String>, config: RuntimeConfig) -> Result<Self> {
        Self::new(namespace, Arc::new(DisabledBackend), config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BackendSyncMode, RuntimeEnvPolicy};

    #[test]
    fn local_runtime_constructs_without_external_backend() {
        let runtime = LocalRuntime::<RuntimeEnvPolicy>::local_only(
            "local-service",
            RuntimeConfig::for_policy::<LocalOnly<RuntimeEnvPolicy>>(),
        )
        .expect("local runtime should not require Redis");

        let state = runtime.subscribe_state().borrow().clone();
        assert_eq!(state.entry_count, 0);
        assert_eq!(
            LocalOnly::<RuntimeEnvPolicy>::backend_sync_mode(),
            BackendSyncMode::Disabled
        );
    }
}
