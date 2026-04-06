//! SSE and WebSocket streaming bridges.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use futures_util::stream::StreamExt;
use futures_util::SinkExt;
use tokio_tungstenite::tungstenite;

use crate::error::DashboardError;
use crate::hosts::find_host;
use crate::state::AppState;

/// Bridge SSE events from host agent to browser.
pub async fn sse_bridge(
    State(state): State<AppState>,
    Path(host_id): Path<String>,
) -> Result<impl IntoResponse, DashboardError> {
    let host = {
        let hosts = state.hosts.read().await;
        find_host(&hosts, &host_id)
            .cloned()
            .ok_or(DashboardError::HostNotFound)?
    };

    let stream = state
        .proxy_client
        .stream_sse(&host.base_url, "/v1/events", host.api_key.as_deref())
        .await?;

    // Convert bytes stream to SSE events
    let event_stream = stream.filter_map(|result| async move {
        match result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                Some(Ok::<_, std::convert::Infallible>(Event::default().data(text)))
            }
            Err(_) => None,
        }
    });

    Ok(Sse::new(event_stream).keep_alive(KeepAlive::default()))
}

/// Bridge WebSocket log stream from host agent to browser.
pub async fn ws_bridge(
    State(state): State<AppState>,
    Path((host_id, engine_id)): Path<(String, String)>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, DashboardError> {
    let host = {
        let hosts = state.hosts.read().await;
        find_host(&hosts, &host_id)
            .cloned()
            .ok_or(DashboardError::HostNotFound)?
    };

    Ok(ws.on_upgrade(move |socket| handle_ws_bridge(socket, host, engine_id)))
}

async fn handle_ws_bridge(
    socket: WebSocket,
    host: crate::types::HostConfig,
    engine_id: String,
) {
    let target_url = format!(
        "{}/v1/engines/{}/logs/ws",
        host.base_url.replace("http", "ws").trim_end_matches('/'),
        engine_id
    );

    tracing::info!(target_url = %target_url, "connecting to upstream websocket");

    // Build request with optional auth
    let request = if let Some(ref api_key) = host.api_key {
        match http::Request::builder()
            .uri(&target_url)
            .header("Authorization", format!("Bearer {api_key}"))
            .body(())
        {
            Ok(req) => req,
            Err(e) => {
                tracing::error!(error = %e, "failed to build upstream request");
                return;
            }
        }
    } else {
        match http::Request::builder().uri(&target_url).body(()) {
            Ok(req) => req,
            Err(e) => {
                tracing::error!(error = %e, "failed to build upstream request");
                return;
            }
        }
    };

    // Connect to upstream
    let (upstream, _) = match tokio_tungstenite::connect_async(request).await {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!(error = %e, "failed to connect to upstream websocket");
            return;
        }
    };

    tracing::info!("upstream websocket connected");

    let (mut up_sink, mut up_stream) = upstream.split();
    let (mut down_sink, mut down_stream) = socket.split();

    // Bidirectional relay
    tokio::select! {
        // Upstream -> downstream
        _ = async {
            while let Some(msg) = up_stream.next().await {
                match msg {
                    Ok(tungstenite::Message::Text(text)) => {
                        if down_sink.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(tungstenite::Message::Binary(data)) => {
                        if down_sink.send(Message::Binary(data.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(tungstenite::Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
        } => {}

        // Downstream -> upstream
        _ = async {
            while let Some(msg) = down_stream.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        if up_sink.send(tungstenite::Message::Text(text.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Binary(data)) => {
                        if up_sink.send(tungstenite::Message::Binary(data.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
            }
        } => {}
    }

    tracing::info!("websocket bridge closed");
}

/// Return 404 for WebSocket upgrade if host not found.
pub async fn ws_bridge_not_found() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "unknown host")
}
