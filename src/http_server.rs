use std::net::SocketAddr;

use anyhow::Result;
use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::clock::VirtualClock;
use crate::state::SharedState;
use crate::{description, eventing, soap};

pub async fn run_http_server(
    addr: SocketAddr,
    state: SharedState,
    clock: VirtualClock,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!(addr = %addr, "HTTP server listening");

    let host = addr.to_string();

    loop {
        tokio::select! {
            accept = listener.accept() => {
                let (stream, _peer) = accept?;
                let io = TokioIo::new(stream);
                let state = state.clone();
                let clock = clock.clone();
                let host = host.clone();

                tokio::spawn(async move {
                    let svc = service_fn(move |req| {
                        let state = state.clone();
                        let clock = clock.clone();
                        let host = host.clone();
                        async move { handle_request(req, &state, &clock, &host).await }
                    });
                    if let Err(e) = http1::Builder::new().serve_connection(io, svc).await {
                        warn!(error = %e, "HTTP connection error");
                    }
                });
            }
            _ = shutdown_rx.changed() => {
                info!("HTTP server shutting down");
                break;
            }
        }
    }

    Ok(())
}

async fn handle_request(
    req: Request<Incoming>,
    state: &SharedState,
    clock: &VirtualClock,
    host: &str,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let method = req.method().as_str().to_uppercase();
    let path = req.uri().path().to_string();

    let result = match (method.as_str(), path.as_str()) {
        ("GET", "/device.xml") => {
            let uuid = state.read().await.device_uuid.clone();
            xml_response(200, description::device_xml(&uuid, host))
        }
        ("GET", "/device-v2.xml") => {
            let uuid = state.read().await.device_uuid.clone();
            xml_response(200, description::device_v2_xml(&uuid, host))
        }
        ("GET", "/scpd/WANIPConn1.xml") => xml_response(200, description::scpd_xml().to_string()),
        ("GET", "/scpd/WANIPConn2.xml") => {
            xml_response(200, description::scpd_v2_xml().to_string())
        }
        ("POST", "/ctl/WANIPConn1") => {
            let soap_action = req
                .headers()
                .get("SOAPAction")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let body_bytes = collect_body(req).await;
            let body_str = String::from_utf8_lossy(&body_bytes).to_string();
            let (status, xml) = soap::handle_soap(
                &body_str,
                soap_action.as_deref(),
                "urn:schemas-upnp-org:service:WANIPConnection:1",
                state,
                clock,
            )
            .await;
            xml_response(status, xml)
        }
        ("POST", "/ctl/WANIPConn2") => {
            let soap_action = req
                .headers()
                .get("SOAPAction")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let body_bytes = collect_body(req).await;
            let body_str = String::from_utf8_lossy(&body_bytes).to_string();
            let (status, xml) = soap::handle_soap(
                &body_str,
                soap_action.as_deref(),
                "urn:schemas-upnp-org:service:WANIPConnection:2",
                state,
                clock,
            )
            .await;
            xml_response(status, xml)
        }
        ("SUBSCRIBE", path) if path.starts_with("/evt/") => {
            let headers = req.headers().clone();
            let (status, extra_headers, body) =
                eventing::handle_subscribe(&headers, state, clock, host).await;
            let mut resp = Response::builder().status(status);
            for (k, v) in &extra_headers {
                resp = resp.header(*k, v.as_str());
            }
            resp.body(Full::new(Bytes::from(body))).unwrap()
        }
        ("UNSUBSCRIBE", path) if path.starts_with("/evt/") => {
            let headers = req.headers().clone();
            let status = eventing::handle_unsubscribe(&headers, state).await;
            Response::builder()
                .status(status)
                .body(Full::new(Bytes::new()))
                .unwrap()
        }
        _ => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from("Not Found")))
            .unwrap(),
    };

    Ok(result)
}

async fn collect_body(req: Request<Incoming>) -> Vec<u8> {
    use http_body_util::BodyExt;
    match req.into_body().collect().await {
        Ok(collected) => collected.to_bytes().to_vec(),
        Err(_) => Vec::new(),
    }
}

fn xml_response(status: u16, body: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .body(Full::new(Bytes::from(body)))
        .unwrap()
}
