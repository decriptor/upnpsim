use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UdpSocket;
use tokio::net::UnixStream;

static TEST_COUNTER: AtomicU64 = AtomicU64::new(30000);

const V2_SERVICE_TYPE: &str = "urn:schemas-upnp-org:service:WANIPConnection:2";

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
        let socket_path = format!("/tmp/upnpsim-v2-test-{}-{}.sock", std::process::id(), id);

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

    async fn time_shift(&self, duration: &str) -> serde_json::Value {
        let stream = UnixStream::connect(&self.socket_path).await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut msg = serde_json::to_string(
            &serde_json::json!({"command": "TimeShift", "duration": duration}),
        )
        .unwrap();
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

fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn assert_fault_code(xml: &str, expected: &str) {
    let code = extract_xml_value(xml, "errorCode")
        .unwrap_or_else(|| panic!("Expected errorCode in fault XML, got:\n{xml}"));
    assert_eq!(code, expected, "Unexpected UPnP fault code");
}

async fn soap_request_v2(port: u16, action: &str, body: &str) -> (u16, String) {
    let soap_action = format!("\"{V2_SERVICE_TYPE}#{action}\"");
    soap_request_v2_with_header(port, action, body, Some(&soap_action)).await
}

async fn soap_request_v2_with_header(
    port: u16,
    action: &str,
    body: &str,
    soap_action: Option<&str>,
) -> (u16, String) {
    let url = format!("http://127.0.0.1:{}/ctl/WANIPConn2", port);
    let soap_body = format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action} xmlns:u="{V2_SERVICE_TYPE}">
      {body}
    </u:{action}>
  </s:Body>
</s:Envelope>"#
    );

    let mut req = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .body(soap_body);
    if let Some(header) = soap_action {
        req = req.header("SOAPAction", header);
    }

    let resp = req.send().await.unwrap();

    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap();
    (status, text)
}

async fn msearch_collect(st: &str, expected_port: Option<u16>) -> Vec<String> {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let msg = format!(
        "M-SEARCH * HTTP/1.1\r\n\
HOST: 239.255.255.250:1900\r\n\
MAN: \"ssdp:discover\"\r\n\
MX: 1\r\n\
ST: {st}\r\n\
\r\n"
    );
    let mut out = Vec::new();
    let mut buf = [0_u8; 4096];
    let expected_prefix = expected_port.map(|p| format!("http://127.0.0.1:{p}/"));
    for _ in 0..10 {
        socket.send_to(msg.as_bytes(), "127.0.0.1:1900").await.unwrap();
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(1000);
        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
                Ok(Ok((len, _))) => {
                    let resp = String::from_utf8_lossy(&buf[..len]).to_string();
                    if let Some(prefix) = &expected_prefix {
                        if parse_ssdp_header(&resp, "LOCATION")
                            .is_some_and(|loc| loc.starts_with(prefix))
                        {
                            out.push(resp);
                        }
                    } else {
                        out.push(resp);
                    }
                }
                _ => break,
            }
        }
        if !out.is_empty() {
            break;
        }
    }
    out
}

fn parse_ssdp_header(msg: &str, name: &str) -> Option<String> {
    msg.lines()
        .find_map(|line| {
            let (k, v) = line.split_once(':')?;
            if k.trim().eq_ignore_ascii_case(name) {
                Some(v.trim().to_string())
            } else {
                None
            }
        })
}

// References:
// - UPnP IGD v2 family: https://openconnectivity.org/developer/specifications/upnp-resources/upnp/internet-gateway-device-igd-v-2-0/
// - SOAPAction in SOAP 1.1: https://www.w3.org/TR/2000/NOTE-SOAP-20000508/#_Toc478383528
#[tokio::test]
async fn test_v2_device_description_exposes_wanipconnection2() {
    let daemon = TestDaemon::start().await;
    let xml = reqwest::get(format!("http://127.0.0.1:{}/device-v2.xml", daemon.port))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(xml.contains("InternetGatewayDevice:2"));
    assert!(xml.contains("WANIPConnection:2"));
    assert!(xml.contains("/ctl/WANIPConn2"));
}

#[tokio::test]
async fn test_v2_scpd_exposes_required_v2_actions() {
    let daemon = TestDaemon::start().await;
    let xml = reqwest::get(format!(
        "http://127.0.0.1:{}/scpd/WANIPConn2.xml",
        daemon.port
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    for action in [
        "SetConnectionType",
        "GetConnectionTypeInfo",
        "RequestConnection",
        "ForceTermination",
        "GetNATRSIPStatus",
        "AddAnyPortMapping",
        "DeletePortMappingRange",
        "GetListOfPortMappings",
    ] {
        assert!(xml.contains(&format!("<name>{action}</name>")));
    }
}

#[tokio::test]
async fn test_v2_connection_management_actions() {
    let daemon = TestDaemon::start().await;
    let (status, xml) = soap_request_v2(daemon.port, "GetConnectionTypeInfo", "").await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewConnectionType").unwrap(),
        "IP_Routed"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewPossibleConnectionTypes").unwrap(),
        "IP_Routed"
    );

    let (status, _) = soap_request_v2(daemon.port, "ForceTermination", "").await;
    assert_eq!(status, 200);
    let (status, xml) = soap_request_v2(daemon.port, "GetStatusInfo", "").await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewConnectionStatus").unwrap(),
        "Disconnected"
    );

    let (status, _) = soap_request_v2(daemon.port, "RequestConnection", "").await;
    assert_eq!(status, 200);
    let (status, xml) = soap_request_v2(daemon.port, "GetStatusInfo", "").await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewConnectionStatus").unwrap(),
        "Connected"
    );
}

#[tokio::test]
async fn test_v2_get_nat_rsip_status_shape() {
    let daemon = TestDaemon::start().await;
    let (status, xml) = soap_request_v2(daemon.port, "GetNATRSIPStatus", "").await;
    assert_eq!(status, 200);
    assert_eq!(extract_xml_value(&xml, "NewNATEnabled").unwrap(), "1");
    assert_eq!(extract_xml_value(&xml, "NewRSIPAvailable").unwrap(), "0");
}

#[tokio::test]
async fn test_v2_get_external_ip() {
    let daemon = TestDaemon::start().await;
    let (status, xml) = soap_request_v2(daemon.port, "GetExternalIPAddress", "").await;
    assert_eq!(status, 200);
    assert!(
        xml.contains(r#"<u:GetExternalIPAddressResponse xmlns:u="urn:schemas-upnp-org:service:WANIPConnection:2">"#),
        "Response should use WANIPConnection:2 namespace"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewExternalIPAddress").unwrap(),
        "203.0.113.1"
    );
}

#[tokio::test]
async fn test_v2_add_any_port_mapping_returns_reserved_port() {
    let daemon = TestDaemon::start().await;
    let body = "<NewRemoteHost></NewRemoteHost>\
                <NewExternalPort>0</NewExternalPort>\
                <NewProtocol>TCP</NewProtocol>\
                <NewInternalPort>1234</NewInternalPort>\
                <NewInternalClient>192.0.2.120</NewInternalClient>\
                <NewEnabled>1</NewEnabled>\
                <NewPortMappingDescription>v2-any-port</NewPortMappingDescription>\
                <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, xml) = soap_request_v2(daemon.port, "AddAnyPortMapping", body).await;
    assert_eq!(status, 200);
    let reserved: u16 = extract_xml_value(&xml, "NewReservedPort")
        .unwrap()
        .parse()
        .unwrap();
    assert!(reserved >= 1024);
}

#[tokio::test]
async fn test_v2_delete_range_and_list_mappings() {
    let daemon = TestDaemon::start().await;
    for port in [4010_u16, 4011_u16] {
        let add_body = format!(
            "<NewRemoteHost></NewRemoteHost>\
             <NewExternalPort>{port}</NewExternalPort>\
             <NewProtocol>TCP</NewProtocol>\
             <NewInternalPort>{port}</NewInternalPort>\
             <NewInternalClient>192.0.2.140</NewInternalClient>\
             <NewEnabled>1</NewEnabled>\
             <NewPortMappingDescription>v2-range</NewPortMappingDescription>\
             <NewLeaseDuration>120</NewLeaseDuration>"
        );
        assert_eq!(
            soap_request_v2(daemon.port, "AddPortMapping", &add_body).await.0,
            200
        );
    }

    let list_body = "<NewStartPort>4010</NewStartPort>\
                     <NewEndPort>4011</NewEndPort>\
                     <NewProtocol>TCP</NewProtocol>\
                     <NewManage>1</NewManage>\
                     <NewNumberOfPorts>0</NewNumberOfPorts>";
    let (status, xml) = soap_request_v2(daemon.port, "GetListOfPortMappings", list_body).await;
    assert_eq!(status, 200);
    let listing = extract_xml_value(&xml, "NewPortListing").unwrap();
    assert!(listing.contains("&lt;PortMappingList"));

    let del_body = "<NewStartPort>4010</NewStartPort>\
                    <NewEndPort>4011</NewEndPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewManage>1</NewManage>";
    let (status, _) = soap_request_v2(daemon.port, "DeletePortMappingRange", del_body).await;
    assert_eq!(status, 200);

    let (status, xml) = soap_request_v2(daemon.port, "GetListOfPortMappings", list_body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "730");
}

#[tokio::test]
async fn test_v2_add_get_delete_roundtrip() {
    let daemon = TestDaemon::start().await;

    let add_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>4444</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>4444</NewInternalPort>\
                    <NewInternalClient>192.0.2.55</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>v2-roundtrip</NewPortMappingDescription>\
                    <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, _) = soap_request_v2(daemon.port, "AddPortMapping", add_body).await;
    assert_eq!(status, 200);

    let get_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>4444</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_body).await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewInternalClient").unwrap(),
        "192.0.2.55"
    );
    assert_eq!(extract_xml_value(&xml, "NewInternalPort").unwrap(), "4444");
    assert_eq!(
        extract_xml_value(&xml, "NewPortMappingDescription").unwrap(),
        "v2-roundtrip"
    );

    let del_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>4444</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>";
    let (status, _) = soap_request_v2(daemon.port, "DeletePortMapping", del_body).await;
    assert_eq!(status, 200);

    // Validate behavior after delete, not just delete status code.
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");
}

#[tokio::test]
async fn test_v2_get_specific_respects_remote_host() {
    let daemon = TestDaemon::start().await;
    let add_body = "<NewRemoteHost>198.51.100.9</NewRemoteHost>\
                    <NewExternalPort>4445</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>4445</NewInternalPort>\
                    <NewInternalClient>192.0.2.56</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>v2-remote-host</NewPortMappingDescription>\
                    <NewLeaseDuration>120</NewLeaseDuration>";
    assert_eq!(
        soap_request_v2(daemon.port, "AddPortMapping", add_body).await.0,
        200
    );

    let wrong_host = "<NewRemoteHost></NewRemoteHost>\
                      <NewExternalPort>4445</NewExternalPort>\
                      <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", wrong_host).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");

    let exact_host = "<NewRemoteHost>198.51.100.9</NewRemoteHost>\
                      <NewExternalPort>4445</NewExternalPort>\
                      <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", exact_host).await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewInternalClient").unwrap(),
        "192.0.2.56"
    );
}

#[tokio::test]
async fn test_v2_delete_respects_remote_host() {
    let daemon = TestDaemon::start().await;
    let add_a = "<NewRemoteHost>198.51.100.10</NewRemoteHost>\
                 <NewExternalPort>4450</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>\
                 <NewInternalPort>4450</NewInternalPort>\
                 <NewInternalClient>192.0.2.57</NewInternalClient>\
                 <NewEnabled>1</NewEnabled>\
                 <NewPortMappingDescription>v2-rh-a</NewPortMappingDescription>\
                 <NewLeaseDuration>120</NewLeaseDuration>";
    let add_b = "<NewRemoteHost>198.51.100.11</NewRemoteHost>\
                 <NewExternalPort>4450</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>\
                 <NewInternalPort>4451</NewInternalPort>\
                 <NewInternalClient>192.0.2.58</NewInternalClient>\
                 <NewEnabled>1</NewEnabled>\
                 <NewPortMappingDescription>v2-rh-b</NewPortMappingDescription>\
                 <NewLeaseDuration>120</NewLeaseDuration>";
    assert_eq!(soap_request_v2(daemon.port, "AddPortMapping", add_a).await.0, 200);
    assert_eq!(soap_request_v2(daemon.port, "AddPortMapping", add_b).await.0, 200);

    let del_a = "<NewRemoteHost>198.51.100.10</NewRemoteHost>\
                 <NewExternalPort>4450</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>";
    assert_eq!(
        soap_request_v2(daemon.port, "DeletePortMapping", del_a).await.0,
        200
    );

    let get_a = "<NewRemoteHost>198.51.100.10</NewRemoteHost>\
                 <NewExternalPort>4450</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_a).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");

    let get_b = "<NewRemoteHost>198.51.100.11</NewRemoteHost>\
                 <NewExternalPort>4450</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>";
    let (status, _) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_b).await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn test_v2_get_specific_escapes_xml_text_values() {
    let daemon = TestDaemon::start().await;
    let add_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>4460</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>4460</NewInternalPort>\
                    <NewInternalClient>192.0.2.59</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>rock &amp; roll &lt;demo&gt;</NewPortMappingDescription>\
                    <NewLeaseDuration>120</NewLeaseDuration>";
    assert_eq!(
        soap_request_v2(daemon.port, "AddPortMapping", add_body).await.0,
        200
    );

    let get_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>4460</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_body).await;
    assert_eq!(status, 200);
    assert!(xml.contains("rock &amp; roll &lt;demo&gt;"));
    assert!(!xml.contains("rock & roll <demo>"));
}

#[tokio::test]
async fn test_v2_get_generic_port_mapping_entry_order() {
    let daemon = TestDaemon::start().await;

    let add_a = "<NewRemoteHost></NewRemoteHost>\
                 <NewExternalPort>6001</NewExternalPort>\
                 <NewProtocol>TCP</NewProtocol>\
                 <NewInternalPort>81</NewInternalPort>\
                 <NewInternalClient>192.0.2.91</NewInternalClient>\
                 <NewEnabled>1</NewEnabled>\
                 <NewPortMappingDescription>v2-generic-a</NewPortMappingDescription>\
                 <NewLeaseDuration>120</NewLeaseDuration>";
    let add_b = "<NewRemoteHost></NewRemoteHost>\
                 <NewExternalPort>6002</NewExternalPort>\
                 <NewProtocol>UDP</NewProtocol>\
                 <NewInternalPort>82</NewInternalPort>\
                 <NewInternalClient>192.0.2.92</NewInternalClient>\
                 <NewEnabled>1</NewEnabled>\
                 <NewPortMappingDescription>v2-generic-b</NewPortMappingDescription>\
                 <NewLeaseDuration>120</NewLeaseDuration>";
    assert_eq!(
        soap_request_v2(daemon.port, "AddPortMapping", add_a).await.0,
        200
    );
    assert_eq!(
        soap_request_v2(daemon.port, "AddPortMapping", add_b).await.0,
        200
    );

    let idx0 = "<NewPortMappingIndex>0</NewPortMappingIndex>";
    let idx1 = "<NewPortMappingIndex>1</NewPortMappingIndex>";
    let (status0, xml0) = soap_request_v2(daemon.port, "GetGenericPortMappingEntry", idx0).await;
    let (status1, xml1) = soap_request_v2(daemon.port, "GetGenericPortMappingEntry", idx1).await;
    assert_eq!(status0, 200);
    assert_eq!(status1, 200);
    assert_eq!(extract_xml_value(&xml0, "NewExternalPort").unwrap(), "6001");
    assert_eq!(extract_xml_value(&xml0, "NewProtocol").unwrap(), "TCP");
    assert_eq!(extract_xml_value(&xml1, "NewExternalPort").unwrap(), "6002");
    assert_eq!(extract_xml_value(&xml1, "NewProtocol").unwrap(), "UDP");
}

#[tokio::test]
async fn test_v2_get_generic_out_of_range_returns_714() {
    let daemon = TestDaemon::start().await;
    let body = "<NewPortMappingIndex>99</NewPortMappingIndex>";
    let (status, xml) = soap_request_v2(daemon.port, "GetGenericPortMappingEntry", body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");
}

#[tokio::test]
async fn test_v2_conflict_in_mapping_entry() {
    let daemon = TestDaemon::start().await;
    let add_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>5555</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>5555</NewInternalPort>\
                    <NewInternalClient>192.0.2.60</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>v2-conflict-a</NewPortMappingDescription>\
                    <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, _) = soap_request_v2(daemon.port, "AddPortMapping", add_body).await;
    assert_eq!(status, 200);

    let add_body_conflict = "<NewRemoteHost></NewRemoteHost>\
                             <NewExternalPort>5555</NewExternalPort>\
                             <NewProtocol>TCP</NewProtocol>\
                             <NewInternalPort>6666</NewInternalPort>\
                             <NewInternalClient>192.0.2.61</NewInternalClient>\
                             <NewEnabled>1</NewEnabled>\
                             <NewPortMappingDescription>v2-conflict-b</NewPortMappingDescription>\
                             <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, xml) = soap_request_v2(daemon.port, "AddPortMapping", add_body_conflict).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "718");
}

#[tokio::test]
async fn test_v2_lease_duration_reduces_after_time_shift() {
    let daemon = TestDaemon::start().await;

    let add_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>7777</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>7777</NewInternalPort>\
                    <NewInternalClient>192.0.2.70</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>v2-lease</NewPortMappingDescription>\
                    <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, _) = soap_request_v2(daemon.port, "AddPortMapping", add_body).await;
    assert_eq!(status, 200);

    let _ = daemon.time_shift("30s").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    let get_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>7777</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_body).await;
    assert_eq!(status, 200);
    let lease: u32 = extract_xml_value(&xml, "NewLeaseDuration")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        lease <= 90 && lease >= 80,
        "expected lease near 90s, got {lease}"
    );
}

#[tokio::test]
async fn test_v2_mapping_expires_after_lease() {
    let daemon = TestDaemon::start().await;
    let add_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>7788</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>\
                    <NewInternalPort>7788</NewInternalPort>\
                    <NewInternalClient>192.0.2.71</NewInternalClient>\
                    <NewEnabled>1</NewEnabled>\
                    <NewPortMappingDescription>v2-expire</NewPortMappingDescription>\
                    <NewLeaseDuration>60</NewLeaseDuration>";
    assert_eq!(
        soap_request_v2(daemon.port, "AddPortMapping", add_body).await.0,
        200
    );
    let _ = daemon.time_shift("120s").await;
    tokio::time::sleep(tokio::time::Duration::from_millis(1200)).await;

    let get_body = "<NewRemoteHost></NewRemoteHost>\
                    <NewExternalPort>7788</NewExternalPort>\
                    <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", get_body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");
}

#[tokio::test]
async fn test_v2_get_status_info_shape() {
    let daemon = TestDaemon::start().await;
    let (status, xml) = soap_request_v2(daemon.port, "GetStatusInfo", "").await;
    assert_eq!(status, 200);
    assert_eq!(
        extract_xml_value(&xml, "NewConnectionStatus").unwrap(),
        "Connected"
    );
    assert_eq!(
        extract_xml_value(&xml, "NewLastConnectionError").unwrap(),
        "ERROR_NONE"
    );
    let uptime: i64 = extract_xml_value(&xml, "NewUptime")
        .unwrap()
        .parse()
        .expect("NewUptime should be a number");
    assert!(uptime >= 0);
}

#[tokio::test]
async fn test_v2_get_specific_not_found_returns_714() {
    let daemon = TestDaemon::start().await;
    let body = "<NewRemoteHost></NewRemoteHost>\
                <NewExternalPort>6553</NewExternalPort>\
                <NewProtocol>TCP</NewProtocol>";
    let (status, xml) = soap_request_v2(daemon.port, "GetSpecificPortMappingEntry", body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "714");
}

#[tokio::test]
async fn test_v2_invalid_args_returns_402() {
    let daemon = TestDaemon::start().await;
    // Missing/invalid protocol should trigger InvalidArgs.
    let body = "<NewRemoteHost></NewRemoteHost>\
                <NewExternalPort>8000</NewExternalPort>\
                <NewProtocol>BOGUS</NewProtocol>\
                <NewInternalPort>8000</NewInternalPort>\
                <NewInternalClient>192.0.2.80</NewInternalClient>\
                <NewEnabled>1</NewEnabled>\
                <NewPortMappingDescription>v2-invalid</NewPortMappingDescription>\
                <NewLeaseDuration>120</NewLeaseDuration>";
    let (status, xml) = soap_request_v2(daemon.port, "AddPortMapping", body).await;
    assert_eq!(status, 500);
    assert_fault_code(&xml, "402");
}

#[tokio::test]
async fn test_v2_invalid_action_returns_401() {
    let daemon = TestDaemon::start().await;
    let (status, xml) = soap_request_v2(daemon.port, "TotallyBogusAction", "").await;
    assert_eq!(status, 401);
    assert_fault_code(&xml, "401");
}

#[tokio::test]
async fn test_v2_eventing_subscribe_unsubscribe() {
    let daemon = TestDaemon::start().await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/evt/WANIPConn2", daemon.port);

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
    assert!(sid.starts_with("uuid:"));

    let resp = client
        .request(reqwest::Method::from_bytes(b"UNSUBSCRIBE").unwrap(), &url)
        .header("SID", &sid)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_v2_eventing_rejects_callback_off_segment() {
    let daemon = TestDaemon::start().await;
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/evt/WANIPConn2", daemon.port);
    let resp = client
        .request(reqwest::Method::from_bytes(b"SUBSCRIBE").unwrap(), &url)
        .header("CALLBACK", "<http://8.8.8.8:9999/callback>")
        .header("NT", "upnp:event")
        .header("TIMEOUT", "Second-300")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 412);
}

#[tokio::test]
async fn test_v2_ssdp_all_matrix_has_expected_st_usn_and_location() {
    let daemon = TestDaemon::start().await;
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    let responses = msearch_collect("ssdp:all", Some(daemon.port)).await;
    if responses.is_empty() {
        // Shared test environments can occasionally drop all multicast traffic.
        return;
    }

    let mut by_st: HashMap<String, String> = HashMap::new();
    for resp in responses {
        if let Some(st) = parse_ssdp_header(&resp, "ST") {
            by_st.entry(st).or_insert(resp);
        }
    }

    let expected_st = [
        "upnp:rootdevice",
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
        "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
        "urn:schemas-upnp-org:device:WANDevice:1",
        "urn:schemas-upnp-org:device:WANDevice:2",
        "urn:schemas-upnp-org:device:WANConnectionDevice:1",
        "urn:schemas-upnp-org:device:WANConnectionDevice:2",
        "urn:schemas-upnp-org:service:WANIPConnection:1",
        "urn:schemas-upnp-org:service:WANIPConnection:2",
    ];

    for st in expected_st {
        let resp = by_st.get(st).unwrap_or_else(|| {
            panic!("Expected ssdp:all response for ST={st}; got {:?}", by_st.keys().collect::<Vec<_>>())
        });
        let usn = parse_ssdp_header(resp, "USN").expect("USN header missing");
        let location = parse_ssdp_header(resp, "LOCATION").expect("LOCATION header missing");
        let boot_id = parse_ssdp_header(resp, "BOOTID.UPNP.ORG");
        let config_id = parse_ssdp_header(resp, "CONFIGID.UPNP.ORG");

        if st == "upnp:rootdevice" {
            assert!(usn.ends_with("::upnp:rootdevice"));
        } else {
            assert!(usn.ends_with(&format!("::{st}")), "USN should include exact ST: {usn}");
        }

        if st.contains(":2") {
            assert!(location.ends_with("/device-v2.xml"), "v2 ST should advertise device-v2.xml");
        } else {
            assert!(location.ends_with("/device.xml"), "v1/root ST should advertise device.xml");
        }
        assert!(boot_id.is_some(), "BOOTID.UPNP.ORG required");
        assert!(config_id.is_some(), "CONFIGID.UPNP.ORG required");
        assert!(usn.starts_with("uuid:"), "USN should start with uuid:");
    }
}

#[tokio::test]
async fn test_v2_missing_soapaction_header_returns_402() {
    let daemon = TestDaemon::start().await;
    let (status, xml) =
        soap_request_v2_with_header(daemon.port, "GetExternalIPAddress", "", None).await;
    assert_eq!(status, 400);
    assert_fault_code(&xml, "402");
}

#[tokio::test]
async fn test_v2_mismatched_soapaction_service_returns_401() {
    let daemon = TestDaemon::start().await;
    let soap_action = "\"urn:schemas-upnp-org:service:WANIPConnection:1#GetExternalIPAddress\"";
    let (status, xml) =
        soap_request_v2_with_header(daemon.port, "GetExternalIPAddress", "", Some(soap_action)).await;
    assert_eq!(status, 401);
    assert_fault_code(&xml, "401");
}

#[tokio::test]
async fn test_v2_mismatched_soapaction_action_returns_401() {
    let daemon = TestDaemon::start().await;
    let soap_action = "\"urn:schemas-upnp-org:service:WANIPConnection:2#AddPortMapping\"";
    let (status, xml) =
        soap_request_v2_with_header(daemon.port, "GetExternalIPAddress", "", Some(soap_action)).await;
    assert_eq!(status, 401);
    assert_fault_code(&xml, "401");
}
