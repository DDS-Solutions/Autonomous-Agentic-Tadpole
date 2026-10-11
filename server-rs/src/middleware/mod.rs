//! @docs ARCHITECTURE:MiddlewarePipeline
//! @docs OPERATIONS_MANUAL:Security
//!
//! ### AI Assist Note
//! **Middleware Hub**: Orchestrates the sequential processing of
//! incoming API requests for the Tadpole OS engine. Enforces the
//! **Security Pipeline**, implementing **Sovereign Authentication**
//! (Bearer token), **Brute-Force Prevention** (Recruitment Rate-Limiting),
//! and **CORS Policy Enforcement**. Coordinates with `request_id` to
//! ensure that all incoming transactions are assigned a unique
//! identifier for end-to-end trace propagation.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: CORS pre-flight rejection (origin mismatch),
//!   401 Unauthorized (invalid `NEURAL_TOKEN`), or 429 Too Many
//!   Requests (Rate-limit exceeded).
//! - **Telemetry Link**: Search for `[mod]` or `[Security]` in
//!   `tracing` logs for block/deny events.
//! - **Trace Scope**: `server-rs::middleware`
//!
//! Middleware Hub — Request processing pipeline
//!
//! Orchestrates the layered security and observability middleware
//! for the Axum server ecosystem.
//!
//! @docs ARCHITECTURE:Networking

pub mod auth;
pub mod boot;

pub mod auth_rate_limit;
pub mod cors;
pub mod deprecation;
pub mod rate_limit;
pub mod request_id;
pub mod security_headers;

use axum::{body::Body, http::Request};
use std::net::SocketAddr;

/// Utility to extract the client IP address, respecting proxy headers.
///
/// ### 🛰️ Proxy Awareness
/// Resolves the client IP by checking the following in order:
/// 1. `CF-Connecting-IP` (Cloudflare)
/// 2. `X-Forwarded-For` (Standard Proxy - takes the first IP)
/// 3. `ConnectInfo<SocketAddr>` (Direct connection)
///
/// Falls back to "unknown" if no IP can be resolved.
pub fn extract_client_ip(req: &Request<Body>) -> String {
    let direct_ip = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|addr| {
            let ip = addr.0.ip();
            match ip {
                std::net::IpAddr::V6(v6) => {
                    if let Some(v4) = v6.to_ipv4_mapped() {
                        std::net::IpAddr::V4(v4)
                    } else {
                        std::net::IpAddr::V6(v6)
                    }
                }
                v4 => v4,
            }
        });

    let trust_proxy = std::env::var("TRUST_PROXY_HEADERS")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);

    if trust_proxy {
        // Only inspect proxy headers if direct socket connection is from a recognized proxy or local/private network
        let is_trusted_peer = direct_ip.as_ref().map(|ip| match ip {
            std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_private(),
            std::net::IpAddr::V6(v6) => v6.is_loopback() || ((v6.segments()[0] & 0xfe00) == 0xfc00),
        }).unwrap_or(true);

        if is_trusted_peer {
            // 1. Check Cloudflare specific header
            if let Some(ip) = req
                .headers()
                .get("cf-connecting-ip")
                .and_then(|v| v.to_str().ok())
            {
                let trimmed = ip.trim();
                if trimmed.parse::<std::net::IpAddr>().is_ok() {
                    return trimmed.to_string();
                }
            }

            // 2. Check X-Forwarded-For (Standard Proxy)
            if let Some(forwarded) = req
                .headers()
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
            {
                for part in forwarded.split(',') {
                    let trimmed = part.trim();
                    if trimmed.parse::<std::net::IpAddr>().is_ok() {
                        return trimmed.to_string();
                    }
                }
            }
        }
    }

    // 3. Socket-level connection info (unspoofable direct source IP)
    direct_ip
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}



// Metadata: [mod]
