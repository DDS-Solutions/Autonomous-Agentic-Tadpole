//! @docs ARCHITECTURE:Security
//!
//! ### AI Assist Note
//! **Rate Limiter**: Orchestrates the communication of ingestion and
//! inference capacity status. Injects standard **X-RateLimit** headers
//! (`Limit`, `Remaining`, `Reset`) into every API response. Enforces
//! a **Sovereign Token Bucket** policy (2000 RPM engine default, tunable via
//! `ENGINE_RATE_LIMIT`). Tracks consumption by client IP to ensure fair
//! resource allocation across the swarm (RLMT-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: 429 Too Many Requests status on legitimate
//!   bursts, or memory leaks in the bucket registry during IP churn.
//! - **Telemetry Link**: Search `[rate_limit]` in tracing logs.
//!   tab of the Tadpole OS dashboard.
//! - **Trace Scope**: `server-rs::middleware::rate_limit`

use axum::{
    body::Body,
    http::{HeaderValue, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::middleware::extract_client_ip;

/// Static fallback values to prevent repeated allocations
static FALLBACK_LIMIT: HeaderValue = HeaderValue::from_static("0");

/// Tracks rate limit buckets by IP.
/// Key: IP address (as string)
/// Value: Arc<Mutex<(tokens, last_refill_timestamp)>>
/// Utilizing `moka::sync::Cache` with thread-safe Mutex for atomic check-and-consume.
static RATE_BUCKETS: Lazy<moka::sync::Cache<String, Arc<Mutex<(f64, Instant)>>>> = Lazy::new(|| {
    moka::sync::Cache::builder()
        .max_capacity(20000)
        .time_to_idle(Duration::from_secs(600)) // 10 minute idle eviction
        .build()
});

static MAX_TOKENS: Lazy<f64> = Lazy::new(|| {
    std::env::var("ENGINE_RATE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000.0)
});

static REFILL_RATE_PER_SEC: Lazy<f64> = Lazy::new(|| *MAX_TOKENS / 60.0);

/// Injects standard rate limit headers into every response and enforces limits.
pub async fn inject_rate_limit_headers(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let client_ip = extract_client_ip(&req);

    // 0. Skip rate limiting for local loopback (desktop app safety)
    if client_ip == "127.0.0.1" || client_ip == "::1" {
        return Ok(next.run(req).await);
    }

    let now = Instant::now();
    let bucket_arc = RATE_BUCKETS.get_with(client_ip.clone(), || {
        Arc::new(Mutex::new((*MAX_TOKENS, now)))
    });

    let (current_tokens, reset_secs, is_exceeded) = {
        let mut bucket = bucket_arc.lock();
        let elapsed = now.duration_since(bucket.1).as_secs_f64();
        bucket.0 = (bucket.0 + elapsed * *REFILL_RATE_PER_SEC).min(*MAX_TOKENS);
        bucket.1 = now;

        let reset = if bucket.0 < *MAX_TOKENS {
            ((*MAX_TOKENS - bucket.0) / *REFILL_RATE_PER_SEC).ceil() as u64
        } else {
            0
        };

        if bucket.0 < 1.0 {
            (bucket.0, reset.max(1), true)
        } else {
            bucket.0 -= 1.0;
            (bucket.0, reset, false)
        }
    };

    if is_exceeded {
        tracing::warn!(
            "🚫 [Security] Rate limit exceeded for IP: {}. Blocked.",
            client_ip
        );
        let mut res = Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .body(Body::empty())
            .unwrap();
        let headers = res.headers_mut();
        headers.insert(
            axum::http::header::RETRY_AFTER,
            HeaderValue::from_str(&reset_secs.to_string())
                .unwrap_or_else(|_| HeaderValue::from_static("1")),
        );
        headers.insert(
            "X-RateLimit-Limit",
            HeaderValue::from_str(&(*MAX_TOKENS as u32).to_string())
                .unwrap_or_else(|_| FALLBACK_LIMIT.clone()),
        );
        headers.insert(
            "X-RateLimit-Remaining",
            HeaderValue::from_static("0"),
        );
        headers.insert(
            "X-RateLimit-Reset",
            HeaderValue::from_str(&reset_secs.to_string())
                .unwrap_or_else(|_| FALLBACK_LIMIT.clone()),
        );
        return Ok(res);
    }

    // 4. Proceed with request
    let mut response = next.run(req).await;

    // 5. Inject headers into response
    let headers = response.headers_mut();
    
    headers.insert(
        "X-RateLimit-Limit",
        HeaderValue::from_str(&(*MAX_TOKENS as u32).to_string())
            .unwrap_or_else(|_| FALLBACK_LIMIT.clone()),
    );
    headers.insert(
        "X-RateLimit-Remaining",
        HeaderValue::from_str(&(current_tokens as u32).to_string())
            .unwrap_or_else(|_| FALLBACK_LIMIT.clone()),
    );
    headers.insert(
        "X-RateLimit-Reset",
        HeaderValue::from_str(&reset_secs.to_string())
            .unwrap_or_else(|_| FALLBACK_LIMIT.clone()),
    );

    Ok(response)
}

/// Deprecated no-op: `moka` handles automated background eviction via `time_to_idle`.
#[deprecated(note = "moka handles automated background eviction via time_to_idle")]
pub fn evict_stale_buckets(_max_age: std::time::Duration) {
    // moka handles this automatically via `time_to_idle`
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};
    use tower::ServiceExt;

    async fn dummy_handler() -> StatusCode {
        StatusCode::OK
    }

    #[tokio::test]
    async fn test_rate_limiting_full_flow() {
        std::env::set_var("TRUST_PROXY_HEADERS", "true");
        let app = Router::new()
            .route("/", get(dummy_handler))
            .layer(axum::middleware::from_fn(inject_rate_limit_headers));

        // 1. Initial request from unknown IP
        let req = Request::builder()
            .uri("/")
            .header("X-Forwarded-For", "192.168.1.50")
            .body(Body::empty())
            .unwrap();
        
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        
        // 2. Verify headers
        let headers = res.headers();
        assert!(headers.contains_key("X-RateLimit-Limit"));
        assert!(headers.contains_key("X-RateLimit-Remaining"));
        assert!(headers.contains_key("X-RateLimit-Reset"));

        // 3. Localhost should skip rate limiting (no headers)
        let req_local = Request::builder()
            .uri("/")
            .header("X-Forwarded-For", "127.0.0.1")
            .body(Body::empty())
            .unwrap();
        
        let res_local = app.clone().oneshot(req_local).await.unwrap();
        assert_eq!(res_local.status(), StatusCode::OK);
        assert!(!res_local.headers().contains_key("X-RateLimit-Limit"));

        // Cleanup
        RATE_BUCKETS.invalidate_all();
        std::env::remove_var("TRUST_PROXY_HEADERS");
    }
}



// Metadata: [rate_limit]
