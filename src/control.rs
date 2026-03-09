use anyhow::Result;
use fancy_duration::FancyDuration;
use serde_json::json;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::clock::VirtualClock;
use crate::protocol::{Request, Response};
use crate::state::{SharedState, TtlMode};

pub async fn run_control_server(
    socket_path: String,
    state: SharedState,
    clock: VirtualClock,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    // Remove stale socket file
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)?;
    info!(path = %socket_path, "control socket listening");

    loop {
        tokio::select! {
            accept = listener.accept() => {
                let (stream, _) = accept?;
                let state = state.clone();
                let clock = clock.clone();
                let shutdown_tx = shutdown_tx.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state, clock, shutdown_tx).await {
                        warn!(error = %e, "control connection error");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => {
                info!("control server received ctrl-c");
                let _ = shutdown_tx.send(true);
                break;
            }
        }
    }

    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    state: SharedState,
    clock: VirtualClock,
    shutdown_tx: watch::Sender<bool>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let response = match serde_json::from_str::<Request>(&line) {
        Ok(req) => handle_request(req, &state, &clock, &shutdown_tx).await,
        Err(e) => Response::err(format!("invalid request: {e}")),
    };

    let mut out = serde_json::to_string(&response)?;
    out.push('\n');
    writer.write_all(out.as_bytes()).await?;
    Ok(())
}

async fn handle_request(
    req: Request,
    state: &SharedState,
    clock: &VirtualClock,
    shutdown_tx: &watch::Sender<bool>,
) -> Response {
    match req {
        Request::Status => {
            let st = state.read().await;
            Response::ok(json!({
                "uptime_secs": clock.uptime_secs(),
                "clock_offset_ms": clock.offset_ms(),
                "ttl_mode": st.ttl_mode,
                "external_ip": st.external_ip,
                "mapping_count": st.mappings.len(),
                "subscription_count": st.subscriptions.len(),
                "device_uuid": st.device_uuid,
            }))
        }
        Request::TimeShift { duration } => {
            match duration.parse::<FancyDuration<Duration>>() {
                Ok(fd) => {
                    clock.shift_forward(fd.duration());
                    Response::ok(json!({
                        "shifted_ms": fd.duration().as_millis() as i64,
                        "new_offset_ms": clock.offset_ms(),
                    }))
                }
                Err(e) => Response::err(format!("invalid duration: {e}")),
            }
        }
        Request::SetTtlMode { mode } => {
            match mode.parse::<TtlMode>() {
                Ok(m) => {
                    state.write().await.ttl_mode = m;
                    Response::ok(json!({ "ttl_mode": m }))
                }
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::SetExternalIp { ip } => {
            state.write().await.external_ip = ip.clone();
            Response::ok(json!({ "external_ip": ip }))
        }
        Request::ListMappings => {
            let st = state.read().await;
            let now = clock.now_secs();
            let mappings: Vec<serde_json::Value> = st
                .mapping_order
                .iter()
                .filter_map(|k| st.mappings.get(k))
                .map(|m| {
                    json!({
                        "protocol": m.protocol,
                        "external_port": m.external_port,
                        "internal_client": m.internal_client,
                        "internal_port": m.internal_port,
                        "enabled": m.enabled,
                        "description": m.description,
                        "lease_duration": m.lease_duration,
                        "remaining_lease": m.remaining_lease(now),
                    })
                })
                .collect();
            Response::ok(json!({ "mappings": mappings }))
        }
        Request::Shutdown => {
            let _ = shutdown_tx.send(true);
            Response::ok_empty()
        }
    }
}
