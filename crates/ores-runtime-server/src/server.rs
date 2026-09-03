use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Serialize;
use tokio::{net::TcpListener, signal};

use crate::{BoxError, Config, ProbeSnapshot, Probes, RuntimeEnv, ServiceContract};

#[derive(Clone)]
struct AppState {
    contract: ServiceContract,
    probes: Probes,
}

#[derive(Serialize)]
struct ServiceBody {
    service: &'static str,
    title: &'static str,
    status: &'static str,
    cache_protocol: &'static str,
    cache_mode: &'static str,
}

#[derive(Serialize)]
struct ProbeBody {
    service: &'static str,
    status: &'static str,
    revision: String,
    cache_protocol: &'static str,
    cache_mode: &'static str,
}

#[derive(Serialize)]
struct ReadyBody {
    service: &'static str,
    status: &'static str,
    #[serde(flatten)]
    probe: ProbeSnapshot,
}

/// Start the runtime server on a newly created Tokio runtime.
///
/// # Errors
///
/// Returns an error if the runtime cannot be created or service initialization,
/// binding, cache synchronization, or serving fails.
pub fn run(contract: ServiceContract) -> Result<(), BoxError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run_async(contract))
}

/// Start the runtime server inside the caller's Tokio runtime.
///
/// # Errors
///
/// Returns an error if configuration, tracing, cache initialization, listener
/// binding, or the Axum server fails.
pub async fn run_async(contract: ServiceContract) -> Result<(), BoxError> {
    let config = Config::from_process(contract)?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_new(&config.log_filter)?)
        .json()
        .try_init()?;

    let runtime_env = RuntimeEnv::start(&config.runtime_env, contract).await?;
    let probes = Probes::new(config.revision, runtime_env);
    let draining = probes.clone();
    let state = AppState { contract, probes };
    let listener = TcpListener::bind(config.bind).await?;
    state.probes.mark_started();
    tracing::info!(
        service = contract.service,
        address = %config.bind,
        cache_protocol = RuntimeEnv::protocol(),
        cache_mode = state.probes.cache_mode(),
        "runtime server listener ready"
    );

    axum::serve(listener, app(state))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            draining.begin_draining();
            draining.shutdown_runtime();
            tokio::time::sleep(Duration::from_millis(250)).await;
        })
        .await?;
    Ok(())
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/livez", get(healthz))
        .route("/readyz", get(readyz))
        .route("/startupz", get(startupz))
        .route("/version", get(version))
        .route("/v1/runtime-config/status", get(runtime_status))
        .with_state(state)
}

async fn root(State(state): State<AppState>) -> Response {
    json_response(
        StatusCode::OK,
        ServiceBody {
            service: state.contract.service,
            title: state.contract.title,
            status: "ok",
            cache_protocol: RuntimeEnv::protocol(),
            cache_mode: state.probes.cache_mode(),
        },
    )
}

async fn healthz(State(state): State<AppState>) -> Response {
    probe_response(StatusCode::OK, "alive", &state)
}

async fn startupz(State(state): State<AppState>) -> Response {
    if state.probes.started() {
        probe_response(StatusCode::OK, "started", &state)
    } else {
        probe_response(StatusCode::SERVICE_UNAVAILABLE, "starting", &state)
    }
}

async fn version(State(state): State<AppState>) -> Response {
    probe_response(StatusCode::OK, "ok", &state)
}

async fn readyz(State(state): State<AppState>) -> Response {
    let probe = state.probes.snapshot().await;
    let (status, label) = if probe.ready {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not_ready")
    };
    json_response(
        status,
        ReadyBody {
            service: state.contract.service,
            status: label,
            probe,
        },
    )
}

async fn runtime_status(State(state): State<AppState>) -> Response {
    let probe = state.probes.snapshot().await;
    json_response(
        StatusCode::OK,
        serde_json::json!({
            "service": state.contract.service,
            "protocol": probe.cache_protocol,
            "mode": probe.cache_mode,
            "ready": probe.ready,
            "stale": probe.cache_stale,
            "revision": probe.cache_revision,
            "entries": probe.cache_entries,
            "maintenanceMode": probe.maintenance_mode,
            "serviceBannerConfigured": state.probes.banner_configured().await,
            "valuesReturned": false
        }),
    )
}

fn probe_response(code: StatusCode, status: &'static str, state: &AppState) -> Response {
    json_response(
        code,
        ProbeBody {
            service: state.contract.service,
            status,
            revision: state.probes.revision().to_owned(),
            cache_protocol: RuntimeEnv::protocol(),
            cache_mode: state.probes.cache_mode(),
        },
    )
}

fn json_response<T: Serialize>(code: StatusCode, body: T) -> Response {
    let mut response = (code, Json(body)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            tokio::select! {
                _ = signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = signal::ctrl_c().await;
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use std::collections::BTreeMap;
    use tower::ServiceExt;

    fn flags() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            ("maintenance-mode", "MAINTENANCE_MODE"),
            ("service-banner", "SERVICE_BANNER"),
        ])
    }

    fn config_map() -> BTreeMap<&'static str, &'static str> {
        BTreeMap::from([
            ("MAINTENANCE_MODE", "runtime.maintenance"),
            ("SERVICE_BANNER", "runtime.banner"),
        ])
    }

    fn contract() -> ServiceContract {
        ServiceContract {
            service: "runtime-server-test",
            title: "Runtime Server Test",
            private_listener: false,
            mutable_keys: &["MAINTENANCE_MODE", "SERVICE_BANNER"],
            secret_keys: &["REDIS_URL"],
            flag_to_env: flags,
            env_to_config: config_map,
        }
    }

    #[tokio::test]
    async fn readiness_reports_local_cache_health() {
        let runtime = RuntimeEnv::start(
            &crate::runtime_env::RuntimeEnvConfig {
                redis_url: None,
                redis_required: false,
                namespace: "runtime-server-test".to_owned(),
                capacity: 8,
                repair_interval: Duration::from_secs(180),
                baseline: BTreeMap::new(),
            },
            contract(),
        )
        .await
        .expect("local runtime should start");
        let probes = Probes::new("test", runtime);
        probes.mark_started();
        let response = app(AppState {
            contract: contract(),
            probes,
        })
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
