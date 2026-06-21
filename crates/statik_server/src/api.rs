//! Minimal management API for statik.
//!
//! Protocol: one newline-delimited JSON request per TCP connection, followed
//! by one newline-delimited JSON response. This is intentionally simple so a
//! local supervisor process can drive statik (e.g. trigger a graceful shutdown
//! so a hot-restore of the real minecraft server can begin) with nothing more
//! than `nc`/`curl`-style tooling.
//!
//! Keep the api listener on localhost — the `token` field is a shared secret,
//! not transport security.
//!
//! Requests:
//!   {"action":"ping"}
//!   {"action":"status"}
//!   {"action":"shutdown","token":"...","reason":"..."}
//!
//! If `config.api.token` is set, every action must include a matching `token`.
//! If it is unset, `ping`/`status` are accepted unauthenticated but `shutdown`
//! is refused (so an unconfigured server cannot be shut down over the network
//! by default).

use std::{net::SocketAddr, sync::Arc};

use serde::{Deserialize, Serialize};
use statik_core::prelude::*;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{broadcast, RwLock},
};

use crate::config::ServerConfig;

#[derive(Debug, Deserialize)]
pub struct ApiRequest {
    pub action: String,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pong: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<ApiStatus>,
}

#[derive(Debug, Serialize)]
struct ApiStatus {
    mc_port: u16,
    motd: String,
    max_players: i32,
    hidden: bool,
}

impl ApiResponse {
    fn ok() -> Self {
        ApiResponse {
            ok: true,
            error: None,
            pong: None,
            status: None,
        }
    }

    fn err(message: impl Into<String>) -> Self {
        ApiResponse {
            ok: false,
            error: Some(message.into()),
            pong: None,
            status: None,
        }
    }
}

/// Handle one api connection: read a single newline-delimited JSON request,
/// authenticate and dispatch it, then write a JSON response.
pub async fn handle(
    stream: TcpStream,
    address: SocketAddr,
    config: Arc<RwLock<ServerConfig>>,
    notify_shutdown: broadcast::Sender<String>,
) {
    debug!("Handling api connection from {}.", address);

    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    match reader.read_line(&mut line).await {
        Ok(0) => {
            debug!(
                "Api connection from {} closed before sending a request.",
                address
            );
            return;
        }
        Ok(_) => {}
        Err(e) => {
            warn!("Api read from {} failed: {e}", address);
            return;
        }
    }

    let request: ApiRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            write_response(
                &mut reader,
                ApiResponse::err(format!("invalid request: {e}")),
            )
            .await;
            return;
        }
    };

    let response = dispatch(request, &config, &notify_shutdown).await;

    write_response(&mut reader, response).await;
}

async fn dispatch(
    request: ApiRequest,
    config: &Arc<RwLock<ServerConfig>>,
    notify_shutdown: &broadcast::Sender<String>,
) -> ApiResponse {
    let cfg = config.read().await;
    let expected_token = cfg.api.token.clone();

    // If a token is configured, every action requires it.
    if let Some(expected) = &expected_token {
        if request.token.as_deref() != Some(expected.as_str()) {
            return ApiResponse::err("unauthorized");
        }
    }

    match request.action.as_str() {
        "ping" => ApiResponse {
            ok: true,
            error: None,
            pong: Some(true),
            status: None,
        },

        "status" => ApiResponse {
            ok: true,
            error: None,
            pong: None,
            status: Some(ApiStatus {
                mc_port: cfg.mc.port,
                motd: cfg.mc.motd.clone(),
                max_players: cfg.mc.max_players,
                hidden: cfg.mc.hidden,
            }),
        },

        "shutdown" => {
            // Mutating action: refuse unless a token is configured (and was
            // supplied, which is checked above).
            if expected_token.is_none() {
                return ApiResponse::err("api.token not configured; shutdown disabled for safety");
            }

            let reason = request
                .reason
                .unwrap_or_else(|| cfg.mc.disconnect_msg.clone());

            drop(cfg);

            info!("Api shutdown requested.");

            match notify_shutdown.send(reason) {
                Ok(_) => ApiResponse::ok(),
                Err(e) => ApiResponse::err(format!("failed to send shutdown: {e}")),
            }
        }

        other => ApiResponse::err(format!("unknown action: {other}")),
    }
}

async fn write_response(writer: &mut BufReader<TcpStream>, response: ApiResponse) {
    let mut out = match serde_json::to_string(&response) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to serialise api response: {e}");
            return;
        }
    };
    out.push('\n');

    let stream = writer.get_mut();
    if let Err(e) = stream.write_all(out.as_bytes()).await {
        warn!("Failed to write api response: {e}");
        return;
    }
    let _ = stream.flush().await;
}
