use std::collections::HashMap;

use crate::clock::VirtualClock;
use crate::state::{PortMapping, SharedState, UPnPError};

const SERVICE_TYPE: &str = "urn:schemas-upnp-org:service:WANIPConnection:1";

/// Parse a SOAP request body and dispatch to the appropriate action handler.
/// Returns the HTTP status code and response body XML.
pub async fn handle_soap(
    body: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let (action, params) = match parse_soap_action(body) {
        Some(v) => v,
        None => return (400, soap_fault(402, "InvalidArgs")),
    };

    match action.as_str() {
        "AddPortMapping" => handle_add_port_mapping(params, state, clock).await,
        "DeletePortMapping" => handle_delete_port_mapping(params, state).await,
        "GetExternalIPAddress" => handle_get_external_ip(state).await,
        "GetGenericPortMappingEntry" => {
            handle_get_generic_port_mapping_entry(params, state, clock).await
        }
        "GetSpecificPortMappingEntry" => {
            handle_get_specific_port_mapping_entry(params, state, clock).await
        }
        "GetStatusInfo" => handle_get_status_info(clock).await,
        _ => (401, soap_fault(401, "InvalidAction")),
    }
}

/// Parse SOAP XML to extract the action name and parameter key/value pairs.
fn parse_soap_action(xml: &str) -> Option<(String, HashMap<String, String>)> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut action_name: Option<String> = None;
    let mut params: HashMap<String, String> = HashMap::new();
    let mut depth = 0u32;
    let mut action_depth = 0u32;
    // depth tracking: 0=root, we look for Body at some depth, then action inside Body
    let mut in_body = false;
    let mut in_action = false;
    let mut current_param: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let name_bytes = name.as_ref();
                let local = local_name(name_bytes);
                depth += 1;
                if local == "Body" {
                    in_body = true;
                } else if in_body && !in_action {
                    action_name = Some(local.to_string());
                    in_action = true;
                    action_depth = depth;
                } else if in_action {
                    current_param = Some(local.to_string());
                }
            }
            Ok(Event::End(_)) => {
                if in_action && depth == action_depth {
                    // Closing the action element itself
                    in_action = false;
                } else if in_action && current_param.is_some() {
                    // End of param element with no text — store empty string
                    let param = current_param.take().unwrap();
                    params.entry(param).or_default();
                } else if in_body && !in_action {
                    in_body = false;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Empty(ref e)) => {
                if in_action {
                    let name = e.name();
                    let name_bytes = name.as_ref();
                    let local = local_name(name_bytes);
                    params.insert(local.to_string(), String::new());
                }
            }
            Ok(Event::Text(ref e)) => {
                if let Some(param) = current_param.take() {
                    let text = e.unescape().unwrap_or_default().to_string();
                    params.insert(param, text);
                }
            }
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    action_name.map(|name| (name, params))
}

fn local_name(qname: &[u8]) -> &str {
    let s = std::str::from_utf8(qname).unwrap_or("");
    // Strip namespace prefix if present (e.g., "u:AddPortMapping" -> "AddPortMapping")
    s.rsplit_once(':').map_or(s, |(_, local)| local)
}

fn param_str(params: &HashMap<String, String>, key: &str) -> Result<String, UPnPError> {
    params
        .get(key)
        .cloned()
        .ok_or(UPnPError::InvalidArgs)
}

fn param_u16(params: &HashMap<String, String>, key: &str) -> Result<u16, UPnPError> {
    params
        .get(key)
        .and_then(|v| v.parse().ok())
        .ok_or(UPnPError::InvalidArgs)
}

fn param_u32(params: &HashMap<String, String>, key: &str) -> Result<u32, UPnPError> {
    params
        .get(key)
        .and_then(|v| v.parse().ok())
        .ok_or(UPnPError::InvalidArgs)
}

fn param_bool(params: &HashMap<String, String>, key: &str) -> Result<bool, UPnPError> {
    let v = params.get(key).ok_or(UPnPError::InvalidArgs)?;
    match v.as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(UPnPError::InvalidArgs),
    }
}

async fn handle_add_port_mapping(
    params: HashMap<String, String>,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let mapping = match (|| -> Result<PortMapping, UPnPError> {
        Ok(PortMapping {
            external_port: param_u16(&params, "NewExternalPort")?,
            protocol: param_str(&params, "NewProtocol")?,
            internal_port: param_u16(&params, "NewInternalPort")?,
            internal_client: param_str(&params, "NewInternalClient")?,
            enabled: param_bool(&params, "NewEnabled")?,
            description: params
                .get("NewPortMappingDescription")
                .cloned()
                .unwrap_or_default(),
            lease_duration: param_u32(&params, "NewLeaseDuration")?,
            created_at_virtual_secs: clock.now_secs(),
        })
    })() {
        Ok(m) => m,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };

    let mut st = state.write().await;
    match st.add_mapping(mapping) {
        Ok(()) => (200, soap_response("AddPortMapping", "")),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_delete_port_mapping(
    params: HashMap<String, String>,
    state: &SharedState,
) -> (u16, String) {
    let protocol = match param_str(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let external_port = match param_u16(&params, "NewExternalPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };

    let mut st = state.write().await;
    match st.delete_mapping(&protocol, external_port) {
        Ok(()) => (200, soap_response("DeletePortMapping", "")),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_get_external_ip(state: &SharedState) -> (u16, String) {
    let st = state.read().await;
    let body = format!(
        "<NewExternalIPAddress>{}</NewExternalIPAddress>",
        st.external_ip
    );
    (200, soap_response("GetExternalIPAddress", &body))
}

async fn handle_get_generic_port_mapping_entry(
    params: HashMap<String, String>,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let index = match param_u16(&params, "NewPortMappingIndex") {
        Ok(v) => v as usize,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };

    let st = state.read().await;
    match st.get_mapping_by_index(index) {
        Ok(m) => (200, soap_response("GetGenericPortMappingEntry", &mapping_xml(m, clock))),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_get_specific_port_mapping_entry(
    params: HashMap<String, String>,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let protocol = match param_str(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let external_port = match param_u16(&params, "NewExternalPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };

    let st = state.read().await;
    match st.get_mapping(&protocol, external_port) {
        Ok(m) => (
            200,
            soap_response("GetSpecificPortMappingEntry", &mapping_xml(m, clock)),
        ),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_get_status_info(clock: &VirtualClock) -> (u16, String) {
    let body = format!(
        "<NewConnectionStatus>Connected</NewConnectionStatus>\
         <NewLastConnectionError>ERROR_NONE</NewLastConnectionError>\
         <NewUptime>{}</NewUptime>",
        clock.uptime_secs()
    );
    (200, soap_response("GetStatusInfo", &body))
}

fn mapping_xml(m: &PortMapping, clock: &VirtualClock) -> String {
    let now = clock.now_secs();
    format!(
        "<NewRemoteHost></NewRemoteHost>\
         <NewExternalPort>{}</NewExternalPort>\
         <NewProtocol>{}</NewProtocol>\
         <NewInternalPort>{}</NewInternalPort>\
         <NewInternalClient>{}</NewInternalClient>\
         <NewEnabled>{}</NewEnabled>\
         <NewPortMappingDescription>{}</NewPortMappingDescription>\
         <NewLeaseDuration>{}</NewLeaseDuration>",
        m.external_port,
        m.protocol,
        m.internal_port,
        m.internal_client,
        if m.enabled { "1" } else { "0" },
        m.description,
        m.remaining_lease(now),
    )
}

fn soap_response(action: &str, body_content: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action}Response xmlns:u="{SERVICE_TYPE}">
      {body_content}
    </u:{action}Response>
  </s:Body>
</s:Envelope>"#,
        action = action,
        SERVICE_TYPE = SERVICE_TYPE,
        body_content = body_content,
    )
}

fn soap_upnp_fault(err: &UPnPError) -> String {
    soap_fault(err.code(), err.description())
}

fn soap_fault(code: u16, description: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <s:Fault>
      <faultcode>s:Client</faultcode>
      <faultstring>UPnPError</faultstring>
      <detail>
        <UPnPError xmlns="urn:schemas-upnp-org:control-1-0">
          <errorCode>{code}</errorCode>
          <errorDescription>{description}</errorDescription>
        </UPnPError>
      </detail>
    </s:Fault>
  </s:Body>
</s:Envelope>"#,
        code = code,
        description = description,
    )
}
