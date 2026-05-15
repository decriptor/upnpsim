use http::StatusCode;
use std::net::{IpAddr, Ipv4Addr};

use crate::clock::VirtualClock;
use crate::state::{GenaSubscription, SharedState};

/// Handle a GENA SUBSCRIBE request. Returns (status_code, headers, body).
pub async fn handle_subscribe(
    headers: &http::HeaderMap,
    state: &SharedState,
    clock: &VirtualClock,
    publisher_host: &str,
) -> (StatusCode, Vec<(&'static str, String)>, String) {
    // Check for SID header (renewal) vs CALLBACK (new subscription)
    if let Some(sid) = headers.get("SID").and_then(|v| v.to_str().ok()) {
        // Renewal
        let st = state.read().await;
        if st.subscriptions.contains_key(sid) {
            let timeout = parse_timeout(headers);
            return (
                StatusCode::OK,
                vec![
                    ("SID", sid.to_string()),
                    ("TIMEOUT", format!("Second-{timeout}")),
                ],
                String::new(),
            );
        }
        return (StatusCode::PRECONDITION_FAILED, vec![], String::new());
    }

    let callback = match headers.get("CALLBACK").and_then(|v| v.to_str().ok()) {
        Some(cb) => {
            // CALLBACK header format: <url>
            cb.trim_matches(|c| c == '<' || c == '>').to_string()
        }
        None => return (StatusCode::PRECONDITION_FAILED, vec![], String::new()),
    };
    match headers.get("NT").and_then(|v| v.to_str().ok()) {
        Some("upnp:event") => {}
        _ => return (StatusCode::PRECONDITION_FAILED, vec![], String::new()),
    }
    if !is_callback_allowed(&callback, publisher_host) {
        return (StatusCode::PRECONDITION_FAILED, vec![], String::new());
    }

    let timeout = parse_timeout(headers);
    let sid = format!("uuid:{}", uuid::Uuid::new_v4());

    let sub = GenaSubscription {
        sid: sid.clone(),
        callback_url: callback,
        timeout_secs: timeout,
        created_at_virtual_secs: clock.now_secs(),
    };

    state.write().await.subscriptions.insert(sid.clone(), sub);

    (
        StatusCode::OK,
        vec![("SID", sid), ("TIMEOUT", format!("Second-{timeout}"))],
        String::new(),
    )
}

fn is_callback_allowed(callback: &str, publisher_host: &str) -> bool {
    let uri = match callback.parse::<http::Uri>() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if uri.scheme_str() != Some("http") {
        return false;
    }
    let callback_host = match uri.host() {
        Some(v) => v,
        None => return false,
    };
    let publisher_ip = parse_host_ip(publisher_host);
    let callback_ip = parse_host_ip(callback_host);
    match (publisher_ip, callback_ip) {
        (Some(pub_ip), Some(cb_ip)) => same_segment_or_private(pub_ip, cb_ip),
        _ => false,
    }
}

fn parse_host_ip(host: &str) -> Option<IpAddr> {
    if host.eq_ignore_ascii_case("localhost") {
        return Some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }
    // host may be host:port
    host.rsplit_once(':')
        .and_then(|(h, _)| h.parse::<IpAddr>().ok())
}

fn same_segment_or_private(a: IpAddr, b: IpAddr) -> bool {
    match (a, b) {
        (IpAddr::V4(a4), IpAddr::V4(b4)) => {
            if a4.is_loopback() || b4.is_loopback() {
                return a4.is_loopback() && b4.is_loopback();
            }
            if !(a4.is_private() && b4.is_private()) {
                return false;
            }
            let ao = a4.octets();
            let bo = b4.octets();
            // Best-effort same segment check for private IPv4 space.
            ao[0] == bo[0] && ao[1] == bo[1] && ao[2] == bo[2]
        }
        _ => false,
    }
}

/// Handle a GENA UNSUBSCRIBE request.
pub async fn handle_unsubscribe(headers: &http::HeaderMap, state: &SharedState) -> StatusCode {
    let sid = match headers.get("SID").and_then(|v| v.to_str().ok()) {
        Some(s) => s.to_string(),
        None => return StatusCode::PRECONDITION_FAILED,
    };

    let mut st = state.write().await;
    if st.subscriptions.remove(&sid).is_some() {
        StatusCode::OK
    } else {
        StatusCode::PRECONDITION_FAILED
    }
}

fn parse_timeout(headers: &http::HeaderMap) -> u32 {
    headers
        .get("TIMEOUT")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.strip_prefix("Second-")
                .and_then(|n| n.parse::<u32>().ok())
        })
        .unwrap_or(1800)
}
