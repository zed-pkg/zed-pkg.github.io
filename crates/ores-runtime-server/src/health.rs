use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use serde::Serialize;

use crate::RuntimeEnv;

#[derive(Clone)]
pub struct Probes {
    started: Arc<AtomicBool>,
    accepting: Arc<AtomicBool>,
    revision: Arc<str>,
    runtime_env: RuntimeEnv,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProbeSnapshot {
    pub started: bool,
    pub ready: bool,
    pub revision: String,
    pub cache_protocol: &'static str,
    pub cache_mode: &'static str,
    pub cache_revision: u64,
    pub cache_entries: usize,
    pub cache_stale: bool,
    pub maintenance_mode: bool,
}

impl Probes {
    pub fn new(revision: impl Into<String>, runtime_env: RuntimeEnv) -> Self {
        Self {
            started: Arc::new(AtomicBool::new(false)),
            accepting: Arc::new(AtomicBool::new(false)),
            revision: Arc::from(revision.into()),
            runtime_env,
        }
    }

    pub fn mark_started(&self) {
        self.started.store(true, Ordering::Release);
        self.accepting.store(true, Ordering::Release);
    }

    pub fn begin_draining(&self) {
        self.accepting.store(false, Ordering::Release);
    }

    pub fn started(&self) -> bool {
        self.started.load(Ordering::Acquire)
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn cache_mode(&self) -> &'static str {
        self.runtime_env.mode().as_str()
    }

    pub async fn snapshot(&self) -> ProbeSnapshot {
        let cache = self.runtime_env.state().await;
        let maintenance_mode = self.runtime_env.maintenance_mode().await;
        let started = self.started();
        let ready = started
            && self.accepting.load(Ordering::Acquire)
            && !maintenance_mode
            && !self.runtime_env.worker_failed()
            && cache.ready
            && !cache.stale;
        ProbeSnapshot {
            started,
            ready,
            revision: self.revision.to_string(),
            cache_protocol: RuntimeEnv::protocol(),
            cache_mode: self.cache_mode(),
            cache_revision: cache.revision,
            cache_entries: cache.entry_count,
            cache_stale: cache.stale,
            maintenance_mode,
        }
    }

    pub async fn banner_configured(&self) -> bool {
        self.runtime_env.banner_configured().await
    }

    pub fn shutdown_runtime(&self) {
        self.runtime_env.shutdown();
    }
}
