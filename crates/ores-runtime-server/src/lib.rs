#![forbid(unsafe_code)]

mod config;
mod contract;
mod health;
mod runtime_env;
mod server;

use std::error::Error;

pub type BoxError = Box<dyn Error + Send + Sync + 'static>;

pub use config::Config;
pub use contract::{MappingFn, ServiceContract};
pub use health::{ProbeSnapshot, Probes};
pub use runtime_env::{CacheMode, RuntimeEnv, RuntimeEnvConfig};
pub use server::{run, run_async};
