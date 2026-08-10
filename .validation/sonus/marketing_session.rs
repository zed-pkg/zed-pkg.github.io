//! Token-blind session status and refresh endpoints for sonusauris.app.
//!
//! The static marketing origin never receives a Supabase access token, refresh
//! token, user id, email, or other account data. It can only ask whether the
//! host-only product session is valid and request a server-side token rotation.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use tracing::warn;

use super::{
    AppState, SESSION_COOKIE, SessionAccessError, current_session, removal_cookie,
    session_needs_mfa,
};
use crate::{auth::AuthError, session::SessionError};

const MARKETING_ORIGIN: &str = "https://sonusauris.app";
const REFRESH_PATH: &str = "/auth/session/refresh";
const REFRESH_AFTER_SECONDS: u64 = 50 * 60;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStatus {
    authenticated: bool,
    refresh_after_seconds: u64,
}

pub(super) fn is_marketing_refresh(
    method: &Method,
    path: &str,
    headers: &HeaderMap,
) -> bool {
    *method == Method::POST
        && path == REFRESH_PATH
        && has_exact_origin(headers, MARKETING_ORIGIN)
}

pub(super) async fn status(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    if !session_origin_allowed(&headers, &state) {
        return rejected_origin();
    }

    match current_session(&state, &jar).await {
        Ok(_) => status_response(&headers, StatusCode::OK, true),
        Err(SessionAccessError::Unauthorized) => {
            let response = status_response(&headers, StatusCode::OK, false);
            (jar.add(removal_cookie(state.config())), response).into_response()
        }
        Err(SessionAccessError::Unavailable) => {
            status_response(&headers, StatusCode::SERVICE_UNAVAILABLE, false)
        }
    }
}

pub(super) async fn refresh(
    State(state): State<AppState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    if !session_origin_allowed(&headers, &state) {
        return rejected_origin();
    }

    match force_refresh(&state, &jar).await {
        Ok(()) => status_response(&headers, StatusCode::OK, true),
        Err(SessionAccessError::Unauthorized) => {
            let response = status_response(&headers, StatusCode::OK, false);
            (jar.add(removal_cookie(state.config())), response).into_response()
        }
        Err(SessionAccessError::Unavailable) => {
            status_response(&headers, StatusCode::SERVICE_UNAVAILABLE, false)
        }
    }
}

async fn force_refresh(state: &AppState, jar: &CookieJar) -> Result<(), SessionAccessError> {
    let raw_token = jar
        .get(SESSION_COOKIE)
        .map(|cookie| cookie.value())
        .ok_or(SessionAccessError::Unauthorized)?;
    let session = state
        .sessions
        .load(raw_token)
        .await
        .map_err(|error| match error {
            SessionError::NotFound | SessionError::Crypto(_) => SessionAccessError::Unauthorized,
            SessionError::Database(_) => SessionAccessError::Unavailable,
        })?;

    let refreshed = match state.supabase.refresh(&session.refresh_token).await {
        Ok(refreshed) => refreshed,
        Err(AuthError::Rejected(_) | AuthError::Unauthorized) => {
            let _ = state.sessions.revoke(session.id).await;
            return Err(SessionAccessError::Unauthorized);
        }
        Err(error) => {
            warn!(?error, "Supabase marketing-session refresh failed");
            return Err(SessionAccessError::Unavailable);
        }
    };

    if refreshed.user.id != session.user_id
        || !state.config.permits(refreshed.user.email.as_deref())
        || session_needs_mfa(&refreshed)
    {
        let _ = state.sessions.revoke(session.id).await;
        return Err(SessionAccessError::Unauthorized);
    }

    state
        .sessions
        .update_tokens(session.id, &refreshed)
        .await
        .map_err(|_| SessionAccessError::Unavailable)?;
    Ok(())
}

fn session_origin_allowed(headers: &HeaderMap, state: &AppState) -> bool {
    has_exact_origin(headers, MARKETING_ORIGIN)
        || has_exact_origin(headers, &state.config.public_origin)
}

fn has_exact_origin(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| origin == expected)
}

fn status_response(
    request_headers: &HeaderMap,
    status: StatusCode,
    authenticated: bool,
) -> Response {
    let mut response = (
        status,
        Json(SessionStatus {
            authenticated,
            refresh_after_seconds: REFRESH_AFTER_SECONDS,
        }),
    )
        .into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    headers.insert(
        HeaderName::from_static("pragma"),
        HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        HeaderName::from_static("cross-origin-resource-policy"),
        HeaderValue::from_static("same-site"),
    );
    headers.append(header::VARY, HeaderValue::from_static("Origin"));

    if has_exact_origin(request_headers, MARKETING_ORIGIN) {
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static(MARKETING_ORIGIN),
        );
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
    }
    response
}

fn rejected_origin() -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "ok": false,
            "error": "cross_site_request_rejected"
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(value: &'static str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static(value));
        headers
    }

    #[test]
    fn refresh_exception_is_exact_to_method_path_and_marketing_origin() {
        let marketing = origin(MARKETING_ORIGIN);
        assert!(is_marketing_refresh(
            &Method::POST,
            REFRESH_PATH,
            &marketing
        ));
        assert!(!is_marketing_refresh(
            &Method::GET,
            REFRESH_PATH,
            &marketing
        ));
        assert!(!is_marketing_refresh(
            &Method::POST,
            "/auth/sign-out",
            &marketing
        ));
        assert!(!is_marketing_refresh(
            &Method::POST,
            REFRESH_PATH,
            &origin("https://evil.example")
        ));
    }

    #[test]
    fn status_contract_is_token_blind_and_credentialed_cors() {
        let payload = serde_json::to_string(&SessionStatus {
            authenticated: true,
            refresh_after_seconds: REFRESH_AFTER_SECONDS,
        })
        .unwrap();
        assert_eq!(
            payload,
            r#"{"authenticated":true,"refreshAfterSeconds":3000}"#
        );
        for forbidden in ["token", "email", "user", "principal", "tenant"] {
            assert!(!payload.contains(forbidden), "{payload}");
        }

        let response = status_response(&origin(MARKETING_ORIGIN), StatusCode::OK, true);
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static(MARKETING_ORIGIN))
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&HeaderValue::from_static("true"))
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store, max-age=0"))
        );
    }
}
