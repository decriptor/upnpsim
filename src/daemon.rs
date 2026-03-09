use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{watch, RwLock};
use tracing::info;

use crate::clock::VirtualClock;
use crate::state::{SimState, TtlMode};

pub async fn run(
    listen: String,
    external_ip: String,
    interface: String,
    ttl_mode: String,
    socket_path: String,
) -> Result<()> {
    let clock = VirtualClock::new();

    let ttl = ttl_mode.parse::<TtlMode>()?;
    let mut sim_state = SimState::new(external_ip);
    sim_state.ttl_mode = ttl;
    let state = Arc::new(RwLock::new(sim_state));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let http_addr: SocketAddr = listen.parse()?;
    let iface: Ipv4Addr = interface.parse()?;
    let http_host = listen.clone();

    // Spawn HTTP server
    let http_state = state.clone();
    let http_clock = clock.clone();
    let http_shutdown = shutdown_rx.clone();
    let http_handle = tokio::spawn(async move {
        if let Err(e) =
            crate::http_server::run_http_server(http_addr, http_state, http_clock, http_shutdown)
                .await
        {
            tracing::error!(error = %e, "HTTP server error");
        }
    });

    // Spawn SSDP
    let ssdp_state = state.clone();
    let ssdp_shutdown = shutdown_rx.clone();
    let ssdp_handle = tokio::spawn(async move {
        if let Err(e) =
            crate::ssdp::run_ssdp(iface, http_host, ssdp_state, ssdp_shutdown).await
        {
            tracing::error!(error = %e, "SSDP error");
        }
    });

    // Spawn TTL reaper
    let reaper_state = state.clone();
    let reaper_clock = clock.clone();
    let mut reaper_shutdown = shutdown_rx.clone();
    let reaper_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let mut st = reaper_state.write().await;
                    if st.ttl_mode == TtlMode::Respect {
                        let now = reaper_clock.now_secs();
                        let reaped = st.reap_expired(now);
                        if reaped > 0 {
                            info!(count = reaped, "reaped expired mappings");
                        }
                        // Also reap expired subscriptions
                        let expired_sids: Vec<String> = st
                            .subscriptions
                            .iter()
                            .filter(|(_, sub)| sub.is_expired(now))
                            .map(|(sid, _)| sid.clone())
                            .collect();
                        for sid in &expired_sids {
                            st.subscriptions.remove(sid);
                        }
                        if !expired_sids.is_empty() {
                            info!(count = expired_sids.len(), "reaped expired subscriptions");
                        }
                    }
                }
                _ = reaper_shutdown.changed() => break,
            }
        }
    });

    // Run control socket server (blocks until shutdown)
    crate::control::run_control_server(socket_path, state, clock, shutdown_tx).await?;

    // Wait for tasks to finish
    let _ = tokio::join!(http_handle, ssdp_handle, reaper_handle);

    info!("daemon shut down");
    Ok(())
}
