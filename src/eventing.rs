use http::StatusCode;

use crate::clock::VirtualClock;
use crate::state::{GenaSubscription, SharedState};

/// Handle a GENA SUBSCRIBE request. Returns (status_code, headers, body).
pub async fn handle_subscribe(
    headers: &http::HeaderMap,
    state: &SharedState,
    clock: &VirtualClock,
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
        vec![
            ("SID", sid),
            ("TIMEOUT", format!("Second-{timeout}")),
        ],
        String::new(),
    )
}

/// Handle a GENA UNSUBSCRIBE request.
pub async fn handle_unsubscribe(
    headers: &http::HeaderMap,
    state: &SharedState,
) -> StatusCode {
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
