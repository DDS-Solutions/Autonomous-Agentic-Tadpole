//! @docs ARCHITECTURE:Security
//!
//! ### AI Assist Note
//! **CORS Policy**: Orchestrates the supported cross-origin request
//! configurations for the Tadpole OS engine. Features **Dynamic Origin
//! Support** via the `ALLOWED_ORIGINS` environment variable (comma-separated).
//! Automatically handles **Deep-Link Protocols** (tauri://localhost) and
//! local development environments (ports 5173, 8000). Enforces **Credential
//! Misalignment Protection**: credentials are automatically disabled if
//! wildcard (`*`) mode is active (CORS-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: `Origin mismatch` preflight errors on custom
//!   Bunker deployments, missing `Authorization` or `x-request-id`
//!   headers in the allowed list, or credential failures on wildcard origins.
//! - **Trace Scope**: `server-rs::middleware::cors`

use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowOrigin, CorsLayer};

#[derive(Debug, Clone, PartialEq, Eq)]
enum CorsOriginPolicy {
    Wildcard,
    LocalDevelopment,
    AllowList(Vec<HeaderValue>),
}

fn resolve_origin_policy(allowed_origins: &str) -> CorsOriginPolicy {
    let allow_any = allowed_origins
        .split(',')
        .map(str::trim)
        .any(|origin| origin == "*");

    if allow_any {
        return CorsOriginPolicy::Wildcard;
    }

    let parsed_origins: Vec<HeaderValue> = allowed_origins
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .filter(|origin| {
            let valid_scheme = origin.starts_with("http://")
                || origin.starts_with("https://")
                || origin.starts_with("tauri://");
            if !valid_scheme {
                tracing::warn!(origin, "Invalid ALLOWED_ORIGINS scheme ignored");
            }
            valid_scheme
        })
        .filter_map(|origin| match HeaderValue::from_str(origin) {
            Ok(value) => Some(value),
            Err(err) => {
                tracing::warn!(origin, error = %err, "Invalid ALLOWED_ORIGINS entry ignored");
                None
            }
        })
        .collect();

    if parsed_origins.is_empty() {
        CorsOriginPolicy::LocalDevelopment
    } else {
        CorsOriginPolicy::AllowList(parsed_origins)
    }
}

/// Configures the CORS policy for the engine.
/// Handles dynamic origins from the ALLOWED_ORIGINS environment variable.
pub fn create_cors_layer() -> CorsLayer {
    let allowed_origins = std::env::var("ALLOWED_ORIGINS").unwrap_or_default();
    let (cors, allow_credentials) = match resolve_origin_policy(&allowed_origins) {
        CorsOriginPolicy::Wildcard => {
            tracing::warn!("CORS wildcard enabled from ALLOWED_ORIGINS=*; credentials disabled");
            (CorsLayer::new().allow_origin(tower_http::cors::Any), false)
        }
        CorsOriginPolicy::LocalDevelopment => {
            tracing::info!("CORS local development origins enabled");
            (
            CorsLayer::new().allow_origin(AllowOrigin::list([
                HeaderValue::from_static("http://localhost:5173"),
                HeaderValue::from_static("http://127.0.0.1:5173"),
                HeaderValue::from_static("http://localhost:3000"),
                HeaderValue::from_static("http://127.0.0.1:3000"),
                HeaderValue::from_static("tauri://localhost"),
            ])),
            true,
            )
        }
        CorsOriginPolicy::AllowList(origins) => {
            tracing::info!(count = origins.len(), "CORS ALLOWED_ORIGINS allow-list enabled");
            (
            CorsLayer::new().allow_origin(AllowOrigin::list(origins)),
            true,
            )
        }
    };

    cors.allow_methods([
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ])
    .allow_headers([
        axum::http::header::CONTENT_TYPE,
        axum::http::header::AUTHORIZATION,
        axum::http::HeaderName::from_static("x-request-id"),
        axum::http::HeaderName::from_static("traceparent"),
    ])
    .allow_credentials(allow_credentials)
}

// Metadata: [cors]

// Metadata: [cors]

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_origins_use_local_development_policy() {
        assert_eq!(resolve_origin_policy(""), CorsOriginPolicy::LocalDevelopment);
    }

    #[test]
    fn wildcard_origin_disables_allow_list() {
        assert_eq!(resolve_origin_policy("*"), CorsOriginPolicy::Wildcard);
    }

    #[test]
    fn comma_separated_origins_become_allow_list() {
        let policy = resolve_origin_policy("http://localhost:5173, https://example.com");
        match policy {
            CorsOriginPolicy::AllowList(origins) => {
                assert_eq!(origins.len(), 2);
                assert_eq!(origins[0], HeaderValue::from_static("http://localhost:5173"));
                assert_eq!(origins[1], HeaderValue::from_static("https://example.com"));
            }
            other => panic!("expected allow-list policy, got {:?}", other),
        }
    }

    #[test]
    fn invalid_origins_are_ignored() {
        let policy = resolve_origin_policy("not valid, https://example.com");
        match policy {
            CorsOriginPolicy::AllowList(origins) => {
                assert_eq!(origins, vec![HeaderValue::from_static("https://example.com")]);
            }
            other => panic!("expected allow-list policy, got {:?}", other),
        }
    }
}
