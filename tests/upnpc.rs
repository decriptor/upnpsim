use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(10000);

fn has_upnpc() -> bool {
    Command::new("upnpc").arg("--help").output().is_ok()
}

/// Run upnpc with the given arguments against the daemon at the given port.
/// Must be called via spawn_blocking to avoid blocking the tokio runtime.
async fn upnpc(port: u16, args: &[&str]) -> std::process::Output {
    let url = format!("http://127.0.0.1:{}/device.xml", port);
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        Command::new("upnpc")
            .arg("-u")
            .arg(&url)
            .args(&args)
            .output()
            .expect("failed to run upnpc")
    })
    .await
    .unwrap()
}

struct TestDaemon {
    port: u16,
    socket_path: String,
    handle: tokio::task::JoinHandle<()>,
}

impl TestDaemon {
    async fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let socket_path = format!("/tmp/upnpsim-upnpc-test-{}-{}.sock", std::process::id(), id);

        let listen = format!("127.0.0.1:{}", port);
        let sp = socket_path.clone();
        let handle = tokio::spawn(async move {
            let _ = upnpsim::daemon::run(
                listen,
                "203.0.113.1".to_string(),
                "0.0.0.0".to_string(),
                "respect".to_string(),
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

/// Helper: add a port mapping via raw SOAP.
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

// ──────────────────────────── upnpc Tests ────────────────────────────

#[tokio::test]
async fn test_upnpc_status() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    let output = upnpc(daemon.port, &["-s"]).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("203.0.113.1"),
        "upnpc -s should show external IP, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_upnpc_add_and_list() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Add a mapping
    let output = upnpc(
        daemon.port,
        &["-a", "192.168.1.50", "80", "9090", "TCP", "0"],
    )
    .await;
    assert!(
        output.status.success(),
        "upnpc -a should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List mappings
    let output = upnpc(daemon.port, &["-l"]).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("9090"),
        "upnpc -l should show external port 9090, got: {}",
        stdout
    );
    assert!(
        stdout.contains("192.168.1.50"),
        "upnpc -l should show internal client, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_upnpc_delete() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Add mapping via SOAP API
    let (status, _) =
        add_mapping_raw(daemon.port, "TCP", 9090, "192.168.1.50", 80, 0, "to-delete").await;
    assert_eq!(status, 200);

    // Delete via upnpc
    let output = upnpc(daemon.port, &["-d", "9090", "TCP"]).await;
    assert!(
        output.status.success(),
        "upnpc -d should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify via SOAP API it's gone
    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>9090</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 500);
    assert!(xml.contains("714") || xml.contains("NoSuchEntryInArray"));
}

#[tokio::test]
async fn test_upnpc_add_verify_via_api() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Add via upnpc
    let output = upnpc(
        daemon.port,
        &["-a", "10.0.0.5", "443", "8443", "TCP", "0"],
    )
    .await;
    assert!(
        output.status.success(),
        "upnpc -a should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify via SOAP GetSpecificPortMappingEntry
    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>8443</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewInternalClient").unwrap(),
        "10.0.0.5"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewInternalPort").unwrap(),
        "443"
    );
}

#[tokio::test]
async fn test_upnpc_api_add_verify_via_upnpc() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Add via SOAP API
    let (status, _) =
        add_mapping_raw(daemon.port, "TCP", 7777, "172.16.0.10", 22, 0, "ssh").await;
    assert_eq!(status, 200);

    // Verify via upnpc -l
    let output = upnpc(daemon.port, &["-l"]).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("7777"),
        "upnpc -l should show port 7777, got: {}",
        stdout
    );
    assert!(
        stdout.contains("172.16.0.10"),
        "upnpc -l should show internal client, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_upnpc_external_ip() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Check default IP
    let output = upnpc(daemon.port, &["-s"]).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("203.0.113.1"));

    // Change IP via control socket
    let resp = daemon
        .control_cmd(json!({"command": "SetExternalIp", "ip": "198.51.100.99"}))
        .await;
    assert_eq!(resp["success"], true);

    // Verify new IP via upnpc
    let output = upnpc(daemon.port, &["-s"]).await;
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("198.51.100.99"),
        "upnpc -s should show new IP 198.51.100.99, got: {}",
        stdout
    );
}

#[tokio::test]
async fn test_upnpc_add_with_duration() {
    if !has_upnpc() {
        eprintln!("upnpc not found, skipping test");
        return;
    }
    let daemon = TestDaemon::start().await;

    // Add mapping with duration
    let output = upnpc(
        daemon.port,
        &["-a", "10.0.0.2", "22", "2222", "TCP", "3600"],
    )
    .await;
    assert!(
        output.status.success(),
        "upnpc -a with duration should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify via SOAP API that lease_duration is set
    let body = "<NewProtocol>TCP</NewProtocol>\
                <NewExternalPort>2222</NewExternalPort>";
    let (status, xml) = soap_request(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 200);

    let lease: u32 = extract_xml_value(&xml, "NewLeaseDuration")
        .unwrap()
        .parse()
        .unwrap();
    // The remaining lease should be close to 3600 (within a few seconds of when we added it)
    assert!(
        lease > 3500 && lease <= 3600,
        "lease_duration should be ~3600, got: {}",
        lease
    );
}
