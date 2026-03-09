use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::state::SharedState;

const SSDP_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const SSDP_PORT: u16 = 1900;

/// All search targets this IGD simulator responds to.
const SEARCH_TARGETS: &[&str] = &[
    "upnp:rootdevice",
    "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
    "urn:schemas-upnp-org:device:WANDevice:1",
    "urn:schemas-upnp-org:device:WANConnectionDevice:1",
    "urn:schemas-upnp-org:service:WANIPConnection:1",
];

pub async fn run_ssdp(
    interface: Ipv4Addr,
    http_host: String,
    state: SharedState,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let socket = create_multicast_socket(interface)?;
    let socket = UdpSocket::from_std(socket.into())?;
    info!(interface = %interface, "SSDP listener started on 239.255.255.250:1900");

    let notify_socket = UdpSocket::bind(SocketAddrV4::new(interface, 0)).await?;

    // Spawn periodic NOTIFY alive
    let state2 = state.clone();
    let http_host2 = http_host.clone();
    let mut shutdown_rx2 = shutdown_rx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let uuid = state2.read().await.device_uuid.clone();
                    send_notify_alive(&notify_socket, &uuid, &http_host2).await;
                }
                _ = shutdown_rx2.changed() => break,
            }
        }
    });

    let mut buf = [0u8; 2048];
    loop {
        tokio::select! {
            result = socket.recv_from(&mut buf) => {
                let (len, src) = result?;
                let msg = String::from_utf8_lossy(&buf[..len]);
                if msg.contains("M-SEARCH") {
                    let st = parse_search_target(&msg);
                    let uuid = state.read().await.device_uuid.clone();
                    handle_msearch(&socket, src, &st, &uuid, &http_host).await;
                }
            }
            _ = shutdown_rx.changed() => {
                info!("SSDP shutting down");
                break;
            }
        }
    }

    Ok(())
}

fn create_multicast_socket(interface: Ipv4Addr) -> Result<Socket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_nonblocking(true)?;

    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, SSDP_PORT).into())?;
    socket.join_multicast_v4(&SSDP_ADDR, &interface)?;
    socket.set_multicast_loop_v4(false)?;

    Ok(socket)
}

fn parse_search_target(msg: &str) -> String {
    for line in msg.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("st:") {
            return line[3..].trim().to_string();
        }
    }
    String::new()
}

async fn handle_msearch(
    socket: &UdpSocket,
    src: SocketAddr,
    search_target: &str,
    uuid: &str,
    http_host: &str,
) {
    let targets: Vec<&str> = if search_target == "ssdp:all" {
        SEARCH_TARGETS.to_vec()
    } else if SEARCH_TARGETS.contains(&search_target) || search_target == format!("uuid:{uuid}") {
        vec![search_target]
    } else {
        return;
    };

    for target in targets {
        let usn = if target == "upnp:rootdevice" {
            format!("uuid:{uuid}::upnp:rootdevice")
        } else if target.starts_with("uuid:") {
            format!("uuid:{uuid}")
        } else {
            format!("uuid:{uuid}::{target}")
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             CACHE-CONTROL: max-age=1800\r\n\
             EXT:\r\n\
             LOCATION: http://{http_host}/device.xml\r\n\
             SERVER: upnpsim/1.0 UPnP/1.0\r\n\
             ST: {target}\r\n\
             USN: {usn}\r\n\
             \r\n"
        );

        if let Err(e) = socket.send_to(response.as_bytes(), src).await {
            debug!(error = %e, "failed to send M-SEARCH response");
        }
    }
}

async fn send_notify_alive(socket: &UdpSocket, uuid: &str, http_host: &str) {
    let dest: SocketAddr = SocketAddrV4::new(SSDP_ADDR, SSDP_PORT).into();

    for target in SEARCH_TARGETS {
        let usn = if *target == "upnp:rootdevice" {
            format!("uuid:{uuid}::upnp:rootdevice")
        } else {
            format!("uuid:{uuid}::{target}")
        };

        let msg = format!(
            "NOTIFY * HTTP/1.1\r\n\
             HOST: 239.255.255.250:1900\r\n\
             CACHE-CONTROL: max-age=1800\r\n\
             LOCATION: http://{http_host}/device.xml\r\n\
             NT: {target}\r\n\
             NTS: ssdp:alive\r\n\
             SERVER: upnpsim/1.0 UPnP/1.0\r\n\
             USN: {usn}\r\n\
             \r\n"
        );

        if let Err(e) = socket.send_to(msg.as_bytes(), dest).await {
            warn!(error = %e, "failed to send NOTIFY alive");
        }
    }
}
