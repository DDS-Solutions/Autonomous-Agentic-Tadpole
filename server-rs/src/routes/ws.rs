//! @docs ARCHITECTURE:Networking
//!
//! ### AI Assist Note
//! **WebSocket Real-Time Bridge**: Orchestrates the bi-directional
//! interface for high-speed logging, engine events, and **Binary Pulse
//! Broadcasting**. Features **Subprotocol Authentication**: enforces
//! the `bearer.<token>` format in the `Sec-WebSocket-Protocol` header
//! to bypass browser-based Authorization header limitations. Implements
//! the **Gemini Live Proxy**: securely pipes multimodal audio streams
//! between the client and Google's backend, protecting server-side API
//! keys. AI agents must ensure the `sec-websocket-protocol` is echoed in
//! the response to prevent RFC 6455 handshake failures (WS-01).
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[ws]` in tracing logs.
//! - **Failure Path**: Unexpected connection drops due to missing
//!   subprotocol acknowledgments, 403 Forbidden on Origin mismatches,
//!   or buffer overflows during high-frequency pulse bursts.
//! - **Trace Scope**: `server-rs::routes::ws`

use crate::error::AppError;
use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension,
};
use futures::{sink::SinkExt, stream::StreamExt};
use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// Maximum concurrent Live Voice WebSocket sessions to bound memory & upstream sockets.
static LIVE_WS_SEMAPHORE: Lazy<Arc<Semaphore>> = Lazy::new(|| Arc::new(Semaphore::new(64)));

pub const WS_LIVE_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const LIVE_MAX_SESSION_BYTES: u64 = 100 * 1024 * 1024;
pub const LIVE_ESTIMATED_COST_PER_SEC: f64 = 0.00005;
pub const WS_CLOSE_QUOTA_EXHAUSTED: u16 = 4402;
pub const WS_CLOSE_SESSION_CAP_EXCEEDED: u16 = 4410;

/// The HTTP upgrade endpoint for WebSockets.
/// Auth is handled by the middleware layer (Bearer header or Sec-WebSocket-Protocol).
/// SEC-01: The client sends `Sec-WebSocket-Protocol: bearer.<token>` because browsers
/// cannot set Authorization headers on WebSocket upgrades. We MUST echo the protocol
/// back in the upgrade response, or the browser will immediately close the connection.
#[tracing::instrument(skip(state, headers, ws), name = "system::ws_upgrade")]
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if tracing::enabled!(tracing::Level::DEBUG) {
        let mut scrubbed_headers = headers.clone();
        if scrubbed_headers.contains_key("sec-websocket-protocol") {
            scrubbed_headers.insert(
                "sec-websocket-protocol",
                axum::http::HeaderValue::from_static("[REDACTED]"),
            );
        }
        if scrubbed_headers.contains_key("authorization") {
            scrubbed_headers.insert(
                "authorization",
                axum::http::HeaderValue::from_static("[REDACTED]"),
            );
        }
        tracing::debug!("📥 WS Handshake Headers: {:?}", scrubbed_headers);
    }
    // CSRF protection: verify the Origin header for WS upgrades
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        let allowed_env = std::env::var("ALLOWED_ORIGINS").unwrap_or_default();
        let allowed: Vec<&str> = allowed_env.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        
        // Always allow internal defaults for dev parity
        let defaults = [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:8000",
            "http://127.0.0.1:8000",
            "tauri://localhost",
            "http://tauri.localhost",
        ];
        
        let mut is_allowed = defaults.contains(&origin);
        if !is_allowed && !allowed.is_empty() {
            is_allowed = allowed.contains(&origin);
        }

        if !is_allowed {
            tracing::warn!("🚫 WS upgrade rejected: unexpected Origin '{}'", origin);
            return Ok(StatusCode::FORBIDDEN.into_response());
        }
    }

    // Extract the subprotocol the client sent (e.g., "bearer.<token>")
    // SEC-01: Split the comma-separated list and pick the first one (usually the bearer token)
    let protocol = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .unwrap_or_default();

    // Check for binary pulse protocol support
    // We check the ORIGINAL header for the pulse flag since it might not be the first one
    let full_protocol_header = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let is_pulse_active = full_protocol_header.contains("tadpole-pulse-v1");

    let proto_kind = protocol.split('.').next().unwrap_or("none");
    tracing::info!(
        "✅ WebSocket handshake authorized via middleware. Protocol kind: {}",
        proto_kind
    );

    // SEC-WS-02: Echo an offered subprotocol. If pulse protocol is offered, select it
    // so that bearer token strings are not echoed back in response headers.
    let ws = if is_pulse_active {
        ws.protocols(["tadpole-pulse-v1"])
    } else if !protocol.is_empty() {
        ws.protocols([protocol])
    } else {
        ws
    };

    Ok(ws
        .on_upgrade(move |socket| handle_socket(socket, state, is_pulse_active))
        .into_response())
}

/// The actual bi-directional WebSocket loop handling messaging.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, is_pulse_active: bool) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to Log entries, Engine events, High-Speed Telemetry, and Audio Streams
    let mut log_rx = state.comms.tx.subscribe();
    let mut event_rx = state.comms.event_tx.subscribe();
    let mut telemetry_rx = state.comms.telemetry_tx.subscribe();
    let mut audio_rx = state.comms.audio_stream_tx.subscribe();
    let mut pulse_rx = state.comms.pulse_tx.subscribe();

    tracing::info!("🔗 High-Performance WebSocket Connected!");

    // Tell the frontend we connected in Rust.
    state.broadcast_sys(
        &format!("Connected to Tadpole OS [Rust Engine v{}]", env!("CARGO_PKG_VERSION")),
        "success",
        None,
    );

    // Spawn a task that constantly reads our global Broadcast channels
    // and instantly forwards to this specific WebSocket connection
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                // 1. Handle System Logs (LogEntry)
                result = log_rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Ok(json_str) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(json_str.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!("WebSocket log receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("WebSocket log channel closed; terminating send task");
                            break;
                        }
                    }
                }

                // 2. Handle Engine Events (serde_json::Value)
                result = event_rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Ok(json_str) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(json_str.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!("WebSocket event receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("WebSocket event channel closed; terminating send task");
                            break;
                        }
                    }
                }

                // 3. Handle High-Speed Telemetry (serde_json::Value)
                result = telemetry_rx.recv() => {
                    match result {
                        Ok(msg) => {
                            if let Ok(json_str) = serde_json::to_string(&msg) {
                                if sender.send(Message::Text(json_str.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!("WebSocket telemetry receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("WebSocket telemetry channel closed; terminating send task");
                            break;
                        }
                    }
                }

                // 4. Handle Real-Time Audio Streams (Vec<u8> binary chunks)
                result = audio_rx.recv() => {
                    match result {
                        Ok(msg) => {
                            // Prepend header 0x01 (Audio)
                            let mut bin = Vec::with_capacity(msg.len() + 1);
                            bin.push(0x01);
                            bin.extend_from_slice(&msg);
                            if sender.send(Message::Binary(bin.into())).await.is_err() {
                                break;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!("WebSocket audio receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("WebSocket audio channel closed; terminating send task");
                            break;
                        }
                    }
                }

                // 5. Handle High-Speed Binary Pulses (MessagePack encoded)
                result = pulse_rx.recv(), if is_pulse_active => {
                    match result {
                        Ok(pulse) => {
                            // MessagePack binary encoding
                            if let Ok(encoded) = rmp_serde::encode::to_vec_named(&*pulse) {
                                // Prepend header 0x02 (Swarm Pulse)
                                let mut bin = Vec::with_capacity(encoded.len() + 1);
                                bin.push(0x02);
                                bin.extend_from_slice(&encoded);
                                if sender.send(Message::Binary(bin.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::debug!("WebSocket pulse receiver lagged by {} messages", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("WebSocket pulse channel closed; terminating send task");
                            break;
                        }
                    }
                }
            }
        }
    });

    // Spawn a task to drain the receiver and detect client disconnects
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Close(_)) => break,
                Ok(Message::Binary(bin)) => {
                    // Logic for incoming audio chunks (STT) could go here
                    tracing::debug!("📥 Received binary message of {} bytes", bin.len());
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    // Exit when either task completes (send failure or client disconnect)
    tokio::select! {
        _ = &mut send_task => { recv_task.abort(); }
        _ = &mut recv_task => { send_task.abort(); }
    }

    tracing::info!("🔗 WebSocket Disconnected.");
}

/// Specialized WebSocket handler for Gemini Live Multimodal API.
/// Proxies client audio/setup to Google's backend to protect API keys,
/// enforcing Privacy Shield, concurrency permits, frame size bounds, and BudgetGuard metering.
#[tracing::instrument(skip(state, headers, ws, auth), name = "system::live_voice_upgrade")]
pub async fn live_voice_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    auth: Option<Extension<crate::middleware::auth::AuthenticatedOperator>>,
) -> Result<impl IntoResponse, AppError> {
    if auth.is_none() {
        return Err(AppError::Unauthorized("Unauthorized".to_string()));
    }

    if state
        .governance
        .privacy_mode
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err(AppError::Forbidden(
            "Live Voice cloud streaming is disabled while Privacy Shield (Local-First) is active."
                .to_string(),
        ));
    }

    let permit = LIVE_WS_SEMAPHORE
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            AppError::RateLimit("Concurrent Live Voice WebSocket connection limit reached".to_string())
        })?;

    // CSRF protection: verify the Origin header for Live Voice upgrades
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        let allowed_env = std::env::var("ALLOWED_ORIGINS").unwrap_or_default();
        let allowed: Vec<&str> = allowed_env
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let defaults = [
            "http://localhost:5173",
            "http://127.0.0.1:5173",
            "http://localhost:8000",
            "http://127.0.0.1:8000",
            "tauri://localhost",
            "http://tauri.localhost",
        ];

        let mut is_allowed = defaults.contains(&origin);
        if !is_allowed && !allowed.is_empty() {
            is_allowed = allowed.contains(&origin);
        }

        if !is_allowed {
            tracing::warn!(
                "🚫 Live Voice WS upgrade rejected: unexpected Origin '{}'",
                origin
            );
            return Ok(StatusCode::FORBIDDEN.into_response());
        }
    }

    let protocol = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("").trim().to_string())
        .unwrap_or_default();

    let ws = if !protocol.is_empty() {
        ws.protocols([protocol])
    } else {
        ws
    };

    let ws = ws
        .max_message_size(16 * 1024 * 1024)
        .max_frame_size(16 * 1024 * 1024);

    Ok(ws
        .on_upgrade(move |socket| handle_live_socket(socket, state, permit))
        .into_response())
}

async fn handle_live_socket(
    mut client_ws: WebSocket,
    state: Arc<AppState>,
    _permit: OwnedSemaphorePermit,
) {
    if state
        .governance
        .privacy_mode
        .load(std::sync::atomic::Ordering::Acquire)
    {
        let _ = client_ws
            .send(Message::Text(
                "Error: Live Voice cloud streaming is disabled while Privacy Shield is active."
                    .into(),
            ))
            .await;
        return;
    }

    let api_key = state
        .registry
        .providers
        .get("google")
        .or_else(|| state.registry.providers.get("gemini"))
        .and_then(|p| p.api_key.clone())
        .filter(|k| !k.trim().is_empty())
        .or_else(|| {
            std::env::var("GOOGLE_API_KEY")
                .ok()
                .filter(|k| !k.trim().is_empty())
        });

    let api_key = match api_key {
        Some(key) => key,
        None => {
            let _ = client_ws
                .send(Message::Text(
                    "Error: Google API key not configured in Provider Settings or GOOGLE_API_KEY environment variable.".into(),
                ))
                .await;
            return;
        }
    };

    // Budget Guard pre-flight check (entity: system:live_voice)
    let live_entity_id = "system:live_voice";
    match state
        .security
        .budget_guard
        .check_budget(live_entity_id, 0.05)
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            tracing::warn!("🚫 [LiveWS] Insufficient budget for Live Voice streaming");
            let _ = client_ws
                .send(Message::Text(
                    "Error: Insufficient budget quota for Live Voice session.".into(),
                ))
                .await;
            return;
        }
        Err(e) => {
            tracing::error!("❌ [LiveWS] Budget check failed (failing closed): {}", e);
            let _ = client_ws
                .send(Message::Text("Error: budget_verification_failed".into()))
                .await;
            return;
        }
    }

    let gemini_url = format!(
        "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.MultimodalLive?key={}",
        api_key
    );

    use tokio_tungstenite::connect_async;
    let connect_future = connect_async::<String>(gemini_url);
    let (gemini_ws, _) = match tokio::time::timeout(WS_LIVE_CONNECT_TIMEOUT, connect_future).await {
        Ok(Ok(conn)) => conn,
        Ok(Err(e)) => {
            let sanitized_err = state.security.secret_redactor.redact(&e.to_string());
            tracing::error!("❌ [LiveWS] Failed to connect to Gemini: {}", sanitized_err);
            let _ = client_ws
                .send(Message::Text("Error: upstream_connect_failed".into()))
                .await;
            return;
        }
        Err(_) => {
            tracing::error!("❌ [LiveWS] Gemini connection attempt timed out after 10 seconds");
            let _ = client_ws
                .send(Message::Text("Error: upstream_connect_timeout".into()))
                .await;
            return;
        }
    };

    relay_live_session(client_ws, state, gemini_ws).await;
}

/// Relays bi-directional frames between client and upstream WebSocket with
/// byte-cap enforcement and periodic BudgetGuard metering.
async fn relay_live_session<S>(
    client_ws: WebSocket,
    state: Arc<AppState>,
    gemini_ws: tokio_tungstenite::WebSocketStream<S>,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let live_entity_id = "system:live_voice";
    let (gemini_sender, mut gemini_receiver) = gemini_ws.split();
    let gemini_sender = Arc::new(tokio::sync::Mutex::new(gemini_sender));
    let (client_sender, mut client_receiver) = client_ws.split();
    let client_sender = Arc::new(tokio::sync::Mutex::new(client_sender));

    tracing::info!("🎙️ [LiveWS] Gemini Live Proxy Established with Budget Guard");

    let client_sender_c2g = client_sender.clone();
    let gemini_sender_c2g = gemini_sender.clone();

    let last_meter_instant = Arc::new(parking_lot::Mutex::new(std::time::Instant::now()));
    let meter_state = state.clone();
    let meter_client_sender = client_sender.clone();
    let last_meter_for_timer = Arc::clone(&last_meter_instant);

    let meter_loop = async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        interval.tick().await;

        loop {
            interval.tick().await;
            let elapsed_secs = {
                let mut last = last_meter_for_timer.lock();
                let elapsed = last.elapsed().as_secs_f64();
                *last = std::time::Instant::now();
                elapsed
            };

            let cost_usd = (elapsed_secs * LIVE_ESTIMATED_COST_PER_SEC).max(0.0001);
            if let Err(e) = meter_state
                .security
                .budget_guard
                .record_usage(live_entity_id, cost_usd)
                .await
            {
                tracing::error!("🚫 [LiveWS] Failed to record usage (failing closed): {}", e);
                break;
            }

            match meter_state
                .security
                .budget_guard
                .check_budget(live_entity_id, 0.01)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    tracing::warn!(
                        "🚫 [LiveWS] Quota exhausted during active stream. Terminating session."
                    );
                    let mut sender = meter_client_sender.lock().await;
                    let _ = sender
                        .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                            code: WS_CLOSE_QUOTA_EXHAUSTED,
                            reason: "Quota exhausted".into(),
                        })))
                        .await;
                    break;
                }
                Err(e) => {
                    tracing::error!(
                        "🚫 [LiveWS] Mid-stream budget check failed (failing closed): {}",
                        e
                    );
                    break;
                }
            }
        }
    };

    let c2g = async move {
        let mut bytes_sent: u64 = 0;

        while let Some(msg) = client_receiver.next().await {
            match msg {
                Ok(axum::extract::ws::Message::Text(t)) => {
                    bytes_sent = bytes_sent.saturating_add(t.len() as u64);
                    if bytes_sent > LIVE_MAX_SESSION_BYTES {
                        tracing::warn!(
                            "🚫 [LiveWS] Session byte cap exceeded ({} bytes). Closing c2g pipe.",
                            bytes_sent
                        );
                        let mut sender = client_sender_c2g.lock().await;
                        let _ = sender
                            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: WS_CLOSE_SESSION_CAP_EXCEEDED,
                                reason: "Session cap exceeded".into(),
                            })))
                            .await;
                        break;
                    }
                    let mut g_sender = gemini_sender_c2g.lock().await;
                    let _ = g_sender
                        .send(tokio_tungstenite::tungstenite::Message::Text(
                            t.as_str().into(),
                        ))
                        .await;
                }
                Ok(axum::extract::ws::Message::Binary(b)) => {
                    bytes_sent = bytes_sent.saturating_add(b.len() as u64);
                    if bytes_sent > LIVE_MAX_SESSION_BYTES {
                        tracing::warn!(
                            "🚫 [LiveWS] Session byte cap exceeded ({} bytes). Closing c2g pipe.",
                            bytes_sent
                        );
                        let mut sender = client_sender_c2g.lock().await;
                        let _ = sender
                            .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: WS_CLOSE_SESSION_CAP_EXCEEDED,
                                reason: "Session cap exceeded".into(),
                            })))
                            .await;
                        break;
                    }
                    let mut g_sender = gemini_sender_c2g.lock().await;
                    let _ = g_sender
                        .send(tokio_tungstenite::tungstenite::Message::Binary(b))
                        .await;
                }
                Ok(axum::extract::ws::Message::Close(_)) => break,
                _ => {}
            }
        }
    };

    let client_sender_g2c = client_sender.clone();
    let g2c = async move {
        let mut downstream_bytes: u64 = 0;
        while let Some(msg) = gemini_receiver.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => {
                    downstream_bytes = downstream_bytes.saturating_add(t.len() as u64);
                    if downstream_bytes > LIVE_MAX_SESSION_BYTES {
                        tracing::warn!(
                            "🚫 [LiveWS] Downstream byte cap exceeded ({} bytes).",
                            downstream_bytes
                        );
                        break;
                    }
                    let mut sender = client_sender_g2c.lock().await;
                    let _ = sender
                        .send(axum::extract::ws::Message::Text(t.to_string().into()))
                        .await;
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(b)) => {
                    downstream_bytes = downstream_bytes.saturating_add(b.len() as u64);
                    if downstream_bytes > LIVE_MAX_SESSION_BYTES {
                        tracing::warn!(
                            "🚫 [LiveWS] Downstream byte cap exceeded ({} bytes).",
                            downstream_bytes
                        );
                        break;
                    }
                    let mut sender = client_sender_g2c.lock().await;
                    let _ = sender.send(axum::extract::ws::Message::Binary(b)).await;
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                _ => {}
            }
        }
    };

    tokio::select! {
        _ = c2g => { tracing::info!("🎙️ [LiveWS] Client closed connection"); }
        _ = g2c => { tracing::info!("🎙️ [LiveWS] Gemini closed connection"); }
        _ = meter_loop => { tracing::info!("🎙️ [LiveWS] Metering terminated session"); }
    }

    // Settle unmetered remainder on disconnect
    let remaining_secs = {
        let last = last_meter_instant.lock();
        last.elapsed().as_secs_f64()
    };
    if remaining_secs >= 0.5 {
        let final_cost = (remaining_secs * LIVE_ESTIMATED_COST_PER_SEC).max(0.0001);
        if let Err(e) = state
            .security
            .budget_guard
            .record_usage(live_entity_id, final_cost)
            .await
        {
            tracing::warn!("⚠️ [LiveWS] Failed to record final session usage: {}", e);
        } else {
            tracing::info!(
                "🎙️ [LiveWS] Final session duration settled: {:.2}s (${:.4})",
                remaining_secs,
                final_cost
            );
        }
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;
    use axum::{routing::get, Router};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message as UpstreamMessage};

    #[tokio::test]
    async fn relay_forwards_both_directions_and_ends_when_client_closes() {
        let upstream_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream = tokio::spawn(async move {
            let (stream, _) = upstream_listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            assert_eq!(
                socket.next().await.unwrap().unwrap(),
                UpstreamMessage::Text("client-frame".into())
            );
            socket
                .send(UpstreamMessage::Binary(vec![1, 2, 3].into()))
                .await
                .unwrap();
            let _ = socket.next().await;
        });

        let state = Arc::new(AppState::new_minimal_mock().await);
        let (finished_tx, mut finished_rx) = tokio::sync::mpsc::channel(1);
        let app = Router::new().route(
            "/",
            get(move |ws: WebSocketUpgrade| {
                let state = state.clone();
                let finished_tx = finished_tx.clone();
                async move {
                    ws.on_upgrade(move |client| async move {
                        let (remote, _) = connect_async(format!("ws://{upstream_addr}"))
                            .await
                            .unwrap();
                        relay_live_session(client, state, remote).await;
                        let _ = finished_tx.send(()).await;
                    })
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let scenario = async {
            let (mut client, _) = connect_async(format!("ws://{address}")).await.unwrap();
            client
                .send(UpstreamMessage::Text("client-frame".into()))
                .await
                .unwrap();
            assert_eq!(
                client.next().await.unwrap().unwrap(),
                UpstreamMessage::Binary(vec![1, 2, 3].into())
            );
            client.close(None).await.unwrap();
            assert_eq!(finished_rx.recv().await, Some(()));
            upstream.await.unwrap();
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), scenario).await;
        server.abort();
        result.unwrap();
    }
}





// Metadata: [ws]
