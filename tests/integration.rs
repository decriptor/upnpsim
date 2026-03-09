use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::sync::atomic::{AtomicU64, Ordering};

use igd::{Gateway, PortMappingProtocol};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn make_gateway(port: u16) -> Gateway {
    let mut control_schema = HashMap::new();
    control_schema.insert(
        "AddPortMapping".into(),
        [
            "NewRemoteHost",
            "NewExternalPort",
            "NewProtocol",
            "NewInternalPort",
            "NewInternalClient",
            "NewEnabled",
            "NewPortMappingDescription",
            "NewLeaseDuration",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    control_schema.insert(
        "DeletePortMapping".into(),
        ["NewRemoteHost", "NewExternalPort", "NewProtocol"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    for action in [
        "GetExternalIPAddress",
        "GetGenericPortMappingEntry",
        "GetSpecificPortMappingEntry",
        "GetStatusInfo",
    ] {
        control_schema.insert(action.to_string(), Vec::new());
    }
    Gateway {
        addr: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), port),
        root_url: format!("http://127.0.0.1:{}", port),
        control_url: "/ctl/WANIPConn1".to_string(),
        control_schema_url: String::new(),
        control_schema,
    }
}

struct TestDaemon {
    port: u16,
    socket_path: String,
    handle: tokio::task::JoinHandle<()>,
}

impl TestDaemon {
    async fn start() -> Self {
        Self::start_with_ttl("respect").await
    }

    async fn start_with_ttl(ttl_mode: &str) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let socket_path = format!("/tmp/upnpsim-test-{}-{}.sock", std::process::id(), id);

        let listen = format!("127.0.0.1:{}", port);
        let sp = socket_path.clone();
        let ttl = ttl_mode.to_string();
        let handle = tokio::spawn(async move {
            let _ = upnpsim::daemon::run(
                listen,
                "203.0.113.1".to_string(),
                "0.0.0.0".to_string(),
                ttl,
                sp,
            )
            .await;
        });

        // Poll until HTTP server is ready
        for _ in 0..200 {
            if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                .await
                .is_ok()
            {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        TestDaemon {
            port,
            socket_path,
            handle,
        }
    }

    async fn control_cmd(&self, request: Value) -> Value {
        let stream = UnixStream::connect(&self.socket_path).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut msg = serde_json::to_string(&request).unwrap();
        msg.push('\n');
        writer.write_all(msg.as_bytes()).await.unwrap();
        let mut reader = BufReader::new(reader);
        let mut response = String::new();
        reader.read_line(&mut response).await.unwrap();
        serde_json::from_str(&response).unwrap()
    }

    async fn time_shift(&self, duration: &str) -> Value {
        self.control_cmd(json!({"command": "TimeShift", "duration": duration}))
            .await
    }

    async fn set_ttl_mode(&self, mode: &str) -> Value {
        self.control_cmd(json!({"command": "SetTtlMode", "mode": mode}))
            .await
    }

    async fn set_external_ip(&self, ip: &str) -> Value {
        self.control_cmd(json!({"command": "SetExternalIp", "ip": ip}))
            .await
    }

}

impl Drop for TestDaemon {
    fn drop(&mut self) {
        self.handle.abort();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// Helper: send a raw SOAP request and return the response body text.
async fn soap_request(port: u16, action: &str, body: &str) -> (u16, String) {
    let url = format!("http://127.0.0.1:{}/ctl/WANIPConn1", port);
    let soap_body = format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:1">
      {body}
    </u:{action}>
  </s:Body>
</s:Envelope>"#
    );

    let resp = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header(
            "SOAPAction",
            format!(
                "\"urn:schemas-upnp-org:service:WANIPConnection:1#{}\"",
                action
            ),
        )
        .body(soap_body)
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    (status, text)
}

/// Extract text between `<tag>` and `</tag>` from XML.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

/// Helper: add a port mapping via raw SOAP (used before igd calls in some tests).
async fn add_mapping_raw(
    port: u16,
    protocol: &str,
    external_port: u16,
    internal_client: &str,
    internal_port: u16,
    lease_duration: u32,
    description: &str,
) -> (u16, String) {
    let body = format!(
        "<NewRemoteHost></NewRemoteHost>\
         <NewExternalPort>{external_port}</NewExternalPort>\
         <NewProtocol>{protocol}</NewProtocol>\
         <NewInternalPort>{internal_port}</NewInternalPort>\
         <NewInternalClient>{internal_client}</NewInternalClient>\
         <NewEnabled>1</NewEnabled>\
         <NewPortMappingDescription>{description}</NewPortMappingDescription>\
         <NewLeaseDuration>{lease_duration}</NewLeaseDuration>"
    );
    soap_request(port, "AddPortMapping", &body).await
}

// ──────────────────────────── Positive Tests ────────────────────────────

#[tokio::test]
async fn test_get_external_ip() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;
    let ip = tokio::task::spawn_blocking(move || make_gateway(port).get_external_ip())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ip, Ipv4Addr::new(203, 0, 113, 1));
}

#[tokio::test]
async fn test_add_port_mapping() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;
    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).add_port(
            PortMappingProtocol::TCP,
            8080,
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 100), 80),
            0,
            "test mapping",
        )
    })
    .await
    .unwrap();
    assert!(result.is_ok(), "add_port failed: {:?}", result.err());
}

#[tokio::test]
async fn test_get_specific_port_mapping() {
    let daemon = TestDaemon::start().await;

    // Add a mapping via raw SOAP
    let (status, _) = add_mapping_raw(
        daemon.port,
        "TCP",
        8080,
        "192.168.1.100",
        80,
        0,
        "specific test",
    )
    .await;
    assert_eq!(status, 200);

    // Query via GetSpecificPortMappingEntry
    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>8080</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewInternalClient").unwrap(),
        "192.168.1.100"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewInternalPort").unwrap(),
        "80"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewPortMappingDescription").unwrap(),
        "specific test"
    );
}

#[tokio::test]
async fn test_get_generic_port_mapping_entry() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    // Add a mapping
    tokio::task::spawn_blocking(move || {
        make_gateway(port).add_port(
            PortMappingProtocol::TCP,
            9090,
            SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 5), 443),
            0,
            "generic test",
        )
    })
    .await
    .unwrap()
    .unwrap();

    // Query index 0
    let port = daemon.port;
    let entry = tokio::task::spawn_blocking(move || {
        make_gateway(port).get_generic_port_mapping_entry(0)
    })
    .await
    .unwrap()
    .unwrap();

    assert_eq!(entry.external_port, 9090);
    assert_eq!(entry.protocol, PortMappingProtocol::TCP);
    assert_eq!(entry.internal_port, 443);
    assert_eq!(entry.internal_client, "10.0.0.5");
    assert_eq!(entry.port_mapping_description, "generic test");
    assert!(entry.enabled);
}

#[tokio::test]
async fn test_delete_port_mapping() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    // Add a mapping
    tokio::task::spawn_blocking(move || {
        make_gateway(port).add_port(
            PortMappingProtocol::TCP,
            7070,
            SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 22),
            0,
            "to delete",
        )
    })
    .await
    .unwrap()
    .unwrap();

    // Delete it
    let port = daemon.port;
    tokio::task::spawn_blocking(move || {
        make_gateway(port).remove_port(PortMappingProtocol::TCP, 7070)
    })
    .await
    .unwrap()
    .unwrap();

    // Verify it's gone via GetSpecificPortMappingEntry
    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>7070</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 500);
    assert!(xml.contains("714") || xml.contains("NoSuchEntryInArray"));
}

#[tokio::test]
async fn test_multiple_mappings_ordered() {
    let daemon = TestDaemon::start().await;

    // Add 3 mappings in order
    let mappings = [
        ("TCP", 1001u16, "10.0.0.1", 80u16, "first"),
        ("TCP", 1002, "10.0.0.2", 81, "second"),
        ("UDP", 1003, "10.0.0.3", 82, "third"),
    ];

    for (proto, ext, client, int, desc) in &mappings {
        let (status, _) =
            add_mapping_raw(daemon.port, proto, *ext, client, *int, 0, desc).await;
        assert_eq!(status, 200, "failed to add mapping {}", desc);
    }

    // Verify each index
    for (i, (proto, ext, client, int, desc)) in mappings.iter().enumerate() {
        let port = daemon.port;
        let idx = i as u32;
        let entry = tokio::task::spawn_blocking(move || {
            make_gateway(port).get_generic_port_mapping_entry(idx)
        })
        .await
        .unwrap()
        .unwrap();

        assert_eq!(entry.external_port, *ext);
        assert_eq!(entry.internal_client, *client);
        assert_eq!(entry.internal_port, *int);
        assert_eq!(entry.port_mapping_description, *desc);
        let expected_proto = if *proto == "TCP" {
            PortMappingProtocol::TCP
        } else {
            PortMappingProtocol::UDP
        };
        assert_eq!(entry.protocol, expected_proto);
    }
}

#[tokio::test]
async fn test_get_status_info() {
    let daemon = TestDaemon::start().await;

    let (status, xml) = soap_request(daemon.port, "GetStatusInfo", "").await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewConnectionStatus").unwrap(),
        "Connected"
    );
    let uptime: i64 = extract_xml_value(&xml, "NewUptime")
        .unwrap()
        .parse()
        .unwrap();
    assert!(uptime >= 0, "uptime should be non-negative, got {}", uptime);
}

#[tokio::test]
async fn test_set_external_ip() {
    let daemon = TestDaemon::start().await;

    // Change IP via control socket
    let resp = daemon.set_external_ip("198.51.100.42").await;
    assert_eq!(resp["success"], true);

    // Verify via get_external_ip
    let port = daemon.port;
    let ip = tokio::task::spawn_blocking(move || make_gateway(port).get_external_ip())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ip, Ipv4Addr::new(198, 51, 100, 42));
}

#[tokio::test]
async fn test_gena_subscribe_unsubscribe() {
    let daemon = TestDaemon::start().await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/evt/WANIPConn1", daemon.port);

    // SUBSCRIBE
    let resp = client
        .request(
            reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(),
            &url,
        )
        .header("CALLBACK", "<http://127.0.0.1:9999/callback>")
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-300")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let sid = resp
        .headers()
        .get("SID")
        .expect("SUBSCRIBE response should have SID header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(sid.starts_with("uuid:"), "SID should start with 'uuid:'");

    let timeout = resp
        .headers()
        .get("TIMEOUT")
        .expect("SUBSCRIBE response should have TIMEOUT header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(timeout.contains("300"));

    // UNSUBSCRIBE
    let resp = client
        .request(
            reqwest::Method::from_bytes(b"UNSUBSCRIBE").unwrap(),
            &url,
        )
        .header("SID", &sid)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify: unsubscribing again should fail
    let resp = client
        .request(
            reqwest::Method::from_bytes(b"UNSUBSCRIBE").unwrap(),
            &url,
        )
        .header("SID", &sid)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 412);
}

// ──────────────────────────── Negative Tests ────────────────────────────

#[tokio::test]
async fn test_add_duplicate_mapping() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    // First add succeeds
    tokio::task::spawn_blocking(move || {
        make_gateway(port).add_port(
            PortMappingProtocol::TCP,
            5555,
            SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 1), 80),
            0,
            "first",
        )
    })
    .await
    .unwrap()
    .unwrap();

    // Second add with same protocol+port should fail
    let port = daemon.port;
    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).add_port(
            PortMappingProtocol::TCP,
            5555,
            SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 81),
            0,
            "duplicate",
        )
    })
    .await
    .unwrap();

    assert!(result.is_err(), "duplicate mapping should fail");
}

#[tokio::test]
async fn test_delete_nonexistent_mapping() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).remove_port(PortMappingProtocol::TCP, 9999)
    })
    .await
    .unwrap();

    assert!(result.is_err(), "deleting nonexistent mapping should fail");
}

#[tokio::test]
async fn test_get_specific_mapping_not_found() {
    let daemon = TestDaemon::start().await;

    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>12345</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 500);
    assert!(xml.contains("714") || xml.contains("NoSuchEntryInArray"));
}

#[tokio::test]
async fn test_get_generic_mapping_out_of_range() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).get_generic_port_mapping_entry(999)
    })
    .await
    .unwrap();

    assert!(
        result.is_err(),
        "querying out-of-range index should return error"
    );
}

// ──────────────────────── TTL / Time-Shift Tests ────────────────────────

#[tokio::test]
async fn test_ttl_expiry_with_time_shift() {
    let daemon = TestDaemon::start().await;

    // Add mapping with 60s lease
    let (status, _) = add_mapping_raw(
        daemon.port,
        "TCP",
        4040,
        "10.0.0.50",
        8080,
        60,
        "ttl test",
    )
    .await;
    assert_eq!(status, 200);

    // Verify it exists
    let port = daemon.port;
    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).get_generic_port_mapping_entry(0)
    })
    .await
    .unwrap();
    assert!(result.is_ok(), "mapping should exist before expiry");

    // Time-shift 120 seconds into the future
    let resp = daemon.time_shift("120s").await;
    assert_eq!(resp["success"], true);

    // Wait for the reaper to run (ticks every 1s)
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // Mapping should be gone
    let port = daemon.port;
    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).get_generic_port_mapping_entry(0)
    })
    .await
    .unwrap();
    assert!(result.is_err(), "mapping should have been reaped after TTL expiry");
}

#[tokio::test]
async fn test_ttl_disrespect_mode() {
    let daemon = TestDaemon::start().await;

    // Add mapping with 60s lease
    let (status, _) = add_mapping_raw(
        daemon.port,
        "TCP",
        4041,
        "10.0.0.51",
        8081,
        60,
        "disrespect test",
    )
    .await;
    assert_eq!(status, 200);

    // Switch to disrespect mode
    let resp = daemon.set_ttl_mode("disrespect").await;
    assert_eq!(resp["success"], true);

    // Time-shift 120 seconds
    let resp = daemon.time_shift("120s").await;
    assert_eq!(resp["success"], true);

    // Wait for reaper tick
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;

    // Mapping should still exist (disrespect mode ignores TTL)
    let port = daemon.port;
    let result = tokio::task::spawn_blocking(move || {
        make_gateway(port).get_generic_port_mapping_entry(0)
    })
    .await
    .unwrap();
    assert!(
        result.is_ok(),
        "mapping should still exist in disrespect mode"
    );
}

// ──────────────────── Control Socket Tests ────────────────────────

#[tokio::test]
async fn test_control_status() {
    let daemon = TestDaemon::start().await;

    let resp = daemon.control_cmd(json!({"command": "Status"})).await;
    assert_eq!(resp["success"], true);
    let data = &resp["data"];
    assert!(data["uptime_secs"].as_i64().unwrap() >= 0);
    assert_eq!(data["ttl_mode"], "respect");
    assert_eq!(data["external_ip"], "203.0.113.1");
    assert_eq!(data["mapping_count"], 0);
    assert_eq!(data["subscription_count"], 0);
    assert!(
        data["device_uuid"].as_str().is_some(),
        "device_uuid should be present"
    );
}

#[tokio::test]
async fn test_control_list_mappings() {
    let daemon = TestDaemon::start().await;

    // Add 2 mappings via SOAP
    let (status, _) =
        add_mapping_raw(daemon.port, "TCP", 5001, "10.0.0.1", 80, 0, "web").await;
    assert_eq!(status, 200);
    let (status, _) =
        add_mapping_raw(daemon.port, "UDP", 5002, "10.0.0.2", 53, 3600, "dns").await;
    assert_eq!(status, 200);

    let resp = daemon
        .control_cmd(json!({"command": "ListMappings"}))
        .await;
    assert_eq!(resp["success"], true);
    let mappings = resp["data"]["mappings"].as_array().unwrap();
    assert_eq!(mappings.len(), 2);

    assert_eq!(mappings[0]["protocol"], "TCP");
    assert_eq!(mappings[0]["external_port"], 5001);
    assert_eq!(mappings[0]["internal_client"], "10.0.0.1");
    assert_eq!(mappings[0]["internal_port"], 80);
    assert_eq!(mappings[0]["description"], "web");
    assert_eq!(mappings[0]["lease_duration"], 0);

    assert_eq!(mappings[1]["protocol"], "UDP");
    assert_eq!(mappings[1]["external_port"], 5002);
    assert_eq!(mappings[1]["internal_client"], "10.0.0.2");
    assert_eq!(mappings[1]["internal_port"], 53);
    assert_eq!(mappings[1]["description"], "dns");
    assert_eq!(mappings[1]["lease_duration"], 3600);
}

#[tokio::test]
async fn test_control_shutdown() {
    let daemon = TestDaemon::start().await;
    let port = daemon.port;

    let resp = daemon
        .control_cmd(json!({"command": "Shutdown"}))
        .await;
    assert_eq!(resp["success"], true);

    // Give the server time to shut down
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // HTTP server should no longer respond
    let result = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap()
        .get(format!("http://127.0.0.1:{}/device.xml", port))
        .send()
        .await;
    assert!(result.is_err(), "HTTP server should be stopped after Shutdown");
}

// ──────────────────── GENA Renewal Test ────────────────────────

#[tokio::test]
async fn test_gena_subscription_renewal() {
    let daemon = TestDaemon::start().await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/evt/WANIPConn1", daemon.port);

    // New subscription
    let resp = client
        .request(reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(), &url)
        .header("CALLBACK", "<http://127.0.0.1:9999/callback>")
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-300")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let sid = resp.headers().get("SID").unwrap().to_str().unwrap().to_string();

    // Renewal with valid SID
    let resp = client
        .request(reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(), &url)
        .header("SID", &sid)
        .header("TIMEOUT", "Second-600")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let renewed_sid = resp.headers().get("SID").unwrap().to_str().unwrap().to_string();
    assert_eq!(sid, renewed_sid, "renewal should return same SID");
    let timeout = resp.headers().get("TIMEOUT").unwrap().to_str().unwrap();
    assert!(timeout.contains("600"), "renewal should echo requested timeout");

    // Renewal with bogus SID → 412
    let resp = client
        .request(reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(), &url)
        .header("SID", "uuid:bogus-nonexistent-sid")
        .header("TIMEOUT", "Second-300")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 412);
}

// ──────────────────── HTTP Endpoint Tests ────────────────────────

#[tokio::test]
async fn test_device_xml() {
    let daemon = TestDaemon::start().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/device.xml", daemon.port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();

    assert!(body.contains("InternetGatewayDevice:1"));
    assert!(body.contains("WANIPConnection:1"));
    assert!(body.contains("<controlURL>/ctl/WANIPConn1</controlURL>"));
    assert!(body.contains("<eventSubURL>/evt/WANIPConn1</eventSubURL>"));
    assert!(body.contains("<UDN>uuid:"));
}

#[tokio::test]
async fn test_scpd_xml() {
    let daemon = TestDaemon::start().await;

    let resp = reqwest::get(format!(
        "http://127.0.0.1:{}/scpd/WANIPConn1.xml",
        daemon.port
    ))
    .await
    .unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.unwrap();

    for action in [
        "AddPortMapping",
        "DeletePortMapping",
        "GetExternalIPAddress",
        "GetGenericPortMappingEntry",
        "GetSpecificPortMappingEntry",
        "GetStatusInfo",
    ] {
        assert!(
            body.contains(&format!("<name>{}</name>", action)),
            "SCPD should contain action {}",
            action
        );
    }
    assert!(body.contains("serviceStateTable"));
}

#[tokio::test]
async fn test_http_404() {
    let daemon = TestDaemon::start().await;

    let resp = reqwest::get(format!("http://127.0.0.1:{}/nonexistent", daemon.port))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_invalid_soap_action() {
    let daemon = TestDaemon::start().await;

    let (status, xml) = soap_request(daemon.port, "BogusAction", "").await;
    assert_eq!(status, 401);
    assert!(xml.contains("401"), "should contain error code 401");
    assert!(
        xml.contains("InvalidAction"),
        "should contain InvalidAction fault"
    );
}
