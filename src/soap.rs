use std::collections::HashMap;

use crate::clock::VirtualClock;
use crate::state::{PortMapping, SharedState, UPnPError};

/// Parse a SOAP request body and dispatch to the appropriate action handler.
/// Returns the HTTP status code and response body XML.
pub async fn handle_soap(
    body: &str,
    soap_action_header: Option<&str>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let (action, params) = match parse_soap_action(body) {
        Some(v) => v,
        None => return (400, soap_fault(402, "InvalidArgs")),
    };

    if let Err((status, xml)) =
        validate_soap_action_header(soap_action_header, service_type, &action)
    {
        return (status, xml);
    }

    let is_v2 = service_type.ends_with(":2");
    match action.as_str() {
        "AddPortMapping" => handle_add_port_mapping(params, service_type, state, clock).await,
        "DeletePortMapping" => handle_delete_port_mapping(params, service_type, state).await,
        "GetExternalIPAddress" => handle_get_external_ip(service_type, state).await,
        "GetGenericPortMappingEntry" => {
            handle_get_generic_port_mapping_entry(params, service_type, state, clock).await
        }
        "GetSpecificPortMappingEntry" => {
            handle_get_specific_port_mapping_entry(params, service_type, state, clock).await
        }
        "GetStatusInfo" => handle_get_status_info(service_type, state, clock).await,
        "SetConnectionType" if is_v2 => handle_set_connection_type(params, service_type, state).await,
        "GetConnectionTypeInfo" if is_v2 => {
            handle_get_connection_type_info(service_type, state).await
        }
        "RequestConnection" if is_v2 => handle_request_connection(service_type, state).await,
        "ForceTermination" if is_v2 => handle_force_termination(service_type, state).await,
        "GetNATRSIPStatus" if is_v2 => handle_get_nat_rsip_status(service_type, state).await,
        "AddAnyPortMapping" if is_v2 => {
            handle_add_any_port_mapping(params, service_type, state, clock).await
        }
        "DeletePortMappingRange" if is_v2 => {
            handle_delete_port_mapping_range(params, service_type, state).await
        }
        "GetListOfPortMappings" if is_v2 => {
            handle_get_list_of_port_mappings(params, service_type, state, clock).await
        }
        _ => (401, soap_fault(401, "InvalidAction")),
    }
}

fn validate_soap_action_header(
    soap_action_header: Option<&str>,
    service_type: &str,
    action: &str,
) -> Result<(), (u16, String)> {
    let raw = match soap_action_header {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return Err((400, soap_fault(402, "InvalidArgs"))),
    };

    let value = raw
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(raw);
    let (service, header_action) = match value.rsplit_once('#') {
        Some(parts) => parts,
        None => return Err((400, soap_fault(402, "InvalidArgs"))),
    };

    if service != service_type || header_action != action {
        return Err((401, soap_fault(401, "InvalidAction")));
    }

    Ok(())
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
    let mut in_body = false;
    let mut in_action = false;
    let mut current_param: Option<String> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
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
                    in_action = false;
                } else if in_action && current_param.is_some() {
                    let param = current_param.take().unwrap();
                    params.entry(param).or_default();
                } else if in_body && !in_action {
                    in_body = false;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Empty(ref e)) => {
                if in_action {
                    params.insert(local_name(e.name().as_ref()).to_string(), String::new());
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
    s.rsplit_once(':').map_or(s, |(_, local)| local)
}

fn param_str(params: &HashMap<String, String>, key: &str) -> Result<String, UPnPError> {
    params.get(key).cloned().ok_or(UPnPError::InvalidArgs)
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

fn param_protocol(params: &HashMap<String, String>, key: &str) -> Result<String, UPnPError> {
    let protocol = param_str(params, key)?;
    match protocol.to_ascii_uppercase().as_str() {
        "TCP" => Ok("TCP".to_string()),
        "UDP" => Ok("UDP".to_string()),
        _ => Err(UPnPError::InvalidArgs),
    }
}

fn is_v2(service_type: &str) -> bool {
    service_type.ends_with(":2")
}

fn normalized_lease_duration(service_type: &str, lease: u32) -> u32 {
    if is_v2(service_type) && lease == 0 {
        604800
    } else {
        lease
    }
}

fn build_mapping(
    params: &HashMap<String, String>,
    service_type: &str,
    clock: &VirtualClock,
) -> Result<PortMapping, UPnPError> {
    Ok(PortMapping {
        remote_host: params.get("NewRemoteHost").cloned().unwrap_or_default(),
        external_port: param_u16(params, "NewExternalPort")?,
        protocol: param_protocol(params, "NewProtocol")?,
        internal_port: param_u16(params, "NewInternalPort")?,
        internal_client: param_str(params, "NewInternalClient")?,
        enabled: param_bool(params, "NewEnabled")?,
        description: params
            .get("NewPortMappingDescription")
            .cloned()
            .unwrap_or_default(),
        lease_duration: normalized_lease_duration(service_type, param_u32(params, "NewLeaseDuration")?),
        created_at_virtual_secs: clock.now_secs(),
    })
}

async fn handle_add_port_mapping(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let mapping = match build_mapping(&params, service_type, clock) {
        Ok(m) => m,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let mut st = state.write().await;
    match st.add_mapping(mapping) {
        Ok(()) => (200, soap_response(service_type, "AddPortMapping", "")),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_add_any_port_mapping(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let requested_port = match param_u16(&params, "NewExternalPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let mut mapping = match build_mapping(&params, service_type, clock) {
        Ok(m) => m,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let mut st = state.write().await;

    let reserved = if requested_port == 0
        || st
            .mappings
            .iter()
            .any(|(k, _)| k.protocol == mapping.protocol && k.external_port == requested_port)
    {
        match find_free_external_port(&st, &mapping.protocol) {
            Some(p) => p,
            None => return (500, soap_fault(501, "ActionFailed")),
        }
    } else {
        requested_port
    };
    mapping.external_port = reserved;

    match st.add_mapping(mapping) {
        Ok(()) => (
            200,
            soap_response(
                service_type,
                "AddAnyPortMapping",
                &format!("<NewReservedPort>{reserved}</NewReservedPort>"),
            ),
        ),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

fn find_free_external_port(st: &crate::state::SimState, protocol: &str) -> Option<u16> {
    (1024..=u16::MAX).find(|port| {
        !st.mappings
            .iter()
            .any(|(k, _)| k.protocol == protocol && k.external_port == *port)
    })
}

async fn handle_delete_port_mapping(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
) -> (u16, String) {
    let protocol = match param_protocol(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let remote_host = params.get("NewRemoteHost").cloned().unwrap_or_default();
    let external_port = match param_u16(&params, "NewExternalPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let mut st = state.write().await;
    match st.delete_mapping(&remote_host, &protocol, external_port) {
        Ok(()) => (200, soap_response(service_type, "DeletePortMapping", "")),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_delete_port_mapping_range(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
) -> (u16, String) {
    let start = match param_u16(&params, "NewStartPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let end = match param_u16(&params, "NewEndPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let protocol = match param_protocol(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    if start > end {
        return (500, soap_upnp_fault(&UPnPError::InconsistentParameters));
    }
    let mut st = state.write().await;
    let keys: Vec<_> = st
        .mapping_order
        .iter()
        .filter(|k| k.protocol == protocol && k.external_port >= start && k.external_port <= end)
        .cloned()
        .collect();
    if keys.is_empty() {
        return (500, soap_upnp_fault(&UPnPError::PortMappingNotFound));
    }
    for key in &keys {
        st.mappings.remove(key);
    }
    st.mapping_order.retain(|k| !keys.contains(k));
    st.bump_system_update_id();
    (200, soap_response(service_type, "DeletePortMappingRange", ""))
}

async fn handle_get_external_ip(service_type: &str, state: &SharedState) -> (u16, String) {
    let st = state.read().await;
    (
        200,
        soap_response(
            service_type,
            "GetExternalIPAddress",
            &format!("<NewExternalIPAddress>{}</NewExternalIPAddress>", st.external_ip),
        ),
    )
}

async fn handle_get_generic_port_mapping_entry(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let index = match param_u16(&params, "NewPortMappingIndex") {
        Ok(v) => v as usize,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let st = state.read().await;
    match st.get_mapping_by_index(index) {
        Ok(m) => (
            200,
            soap_response(service_type, "GetGenericPortMappingEntry", &mapping_xml(m, clock)),
        ),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_get_specific_port_mapping_entry(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let protocol = match param_protocol(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let remote_host = params.get("NewRemoteHost").cloned().unwrap_or_default();
    let external_port = match param_u16(&params, "NewExternalPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let st = state.read().await;
    match st.get_mapping(&remote_host, &protocol, external_port) {
        Ok(m) => (
            200,
            soap_response(service_type, "GetSpecificPortMappingEntry", &mapping_xml(m, clock)),
        ),
        Err(e) => (500, soap_upnp_fault(&e)),
    }
}

async fn handle_get_list_of_port_mappings(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let start = match param_u16(&params, "NewStartPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let end = match param_u16(&params, "NewEndPort") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let protocol = match param_protocol(&params, "NewProtocol") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    let limit = param_u32(&params, "NewNumberOfPorts").unwrap_or(0);
    if start > end {
        return (500, soap_upnp_fault(&UPnPError::InconsistentParameters));
    }

    let st = state.read().await;
    let mut mappings: Vec<&PortMapping> = st
        .mapping_order
        .iter()
        .filter(|k| k.protocol == protocol && k.external_port >= start && k.external_port <= end)
        .filter_map(|k| st.mappings.get(k))
        .collect();
    if limit > 0 {
        mappings.truncate(limit as usize);
    }
    if mappings.is_empty() {
        return (500, soap_upnp_fault(&UPnPError::PortMappingNotFound));
    }
    let listing = port_listing_xml(&mappings, clock);
    let body = format!("<NewPortListing>{}</NewPortListing>", xml_escape(&listing));
    (200, soap_response(service_type, "GetListOfPortMappings", &body))
}

fn port_listing_xml(mappings: &[&PortMapping], clock: &VirtualClock) -> String {
    let mut out =
        "<PortMappingList xmlns=\"urn:schemas-upnp-org:gw:WANIPConnection\">".to_string();
    let now = clock.now_secs();
    for m in mappings {
        out.push_str(&format!(
            "<PortMappingEntry NewRemoteHost=\"{}\" NewExternalPort=\"{}\" NewProtocol=\"{}\" NewInternalPort=\"{}\" NewInternalClient=\"{}\" NewEnabled=\"{}\" NewPortMappingDescription=\"{}\" NewLeaseDuration=\"{}\" />",
            xml_attr_escape(&m.remote_host),
            m.external_port,
            m.protocol,
            m.internal_port,
            xml_attr_escape(&m.internal_client),
            if m.enabled { "1" } else { "0" },
            xml_attr_escape(&m.description),
            m.remaining_lease(now),
        ));
    }
    out.push_str("</PortMappingList>");
    out
}

async fn handle_get_status_info(
    service_type: &str,
    state: &SharedState,
    clock: &VirtualClock,
) -> (u16, String) {
    let st = state.read().await;
    let body = format!(
        "<NewConnectionStatus>{}</NewConnectionStatus>\
         <NewLastConnectionError>ERROR_NONE</NewLastConnectionError>\
         <NewUptime>{}</NewUptime>",
        st.connection_status,
        clock.uptime_secs()
    );
    (200, soap_response(service_type, "GetStatusInfo", &body))
}

async fn handle_set_connection_type(
    params: HashMap<String, String>,
    service_type: &str,
    state: &SharedState,
) -> (u16, String) {
    let new_type = match param_str(&params, "NewConnectionType") {
        Ok(v) => v,
        Err(e) => return (500, soap_upnp_fault(&e)),
    };
    if !matches!(new_type.as_str(), "Unconfigured" | "IP_Routed" | "IP_Bridged") {
        return (500, soap_fault(601, "ArgumentValueOutOfRange"));
    }
    let mut st = state.write().await;
    if !matches!(st.connection_status.as_str(), "Disconnected" | "Unconfigured") {
        return (500, soap_upnp_fault(&UPnPError::InactiveConnectionStateRequired));
    }
    st.connection_type = new_type.clone();
    if new_type == "Unconfigured" {
        st.connection_status = "Unconfigured".to_string();
    }
    (200, soap_response(service_type, "SetConnectionType", ""))
}

async fn handle_get_connection_type_info(service_type: &str, state: &SharedState) -> (u16, String) {
    let st = state.read().await;
    let body = format!(
        "<NewConnectionType>{}</NewConnectionType>\
         <NewPossibleConnectionTypes>{}</NewPossibleConnectionTypes>",
        st.connection_type, st.possible_connection_types
    );
    (200, soap_response(service_type, "GetConnectionTypeInfo", &body))
}

async fn handle_request_connection(service_type: &str, state: &SharedState) -> (u16, String) {
    let mut st = state.write().await;
    if st.connection_type != "IP_Routed" {
        return (500, soap_upnp_fault(&UPnPError::ConnectionNotConfigured));
    }
    st.connection_status = "Connected".to_string();
    (200, soap_response(service_type, "RequestConnection", ""))
}

async fn handle_force_termination(service_type: &str, state: &SharedState) -> (u16, String) {
    let mut st = state.write().await;
    if st.connection_type != "IP_Routed" {
        return (500, soap_upnp_fault(&UPnPError::ConnectionNotConfigured));
    }
    st.connection_status = "Disconnected".to_string();
    (200, soap_response(service_type, "ForceTermination", ""))
}

async fn handle_get_nat_rsip_status(service_type: &str, state: &SharedState) -> (u16, String) {
    let st = state.read().await;
    let body = format!(
        "<NewRSIPAvailable>{}</NewRSIPAvailable>\
         <NewNATEnabled>{}</NewNATEnabled>",
        bool_to_upnp(st.rsip_available),
        bool_to_upnp(st.nat_enabled)
    );
    (200, soap_response(service_type, "GetNATRSIPStatus", &body))
}

fn bool_to_upnp(v: bool) -> &'static str {
    if v { "1" } else { "0" }
}

fn mapping_xml(m: &PortMapping, clock: &VirtualClock) -> String {
    let now = clock.now_secs();
    format!(
        "<NewRemoteHost>{}</NewRemoteHost>\
         <NewExternalPort>{}</NewExternalPort>\
         <NewProtocol>{}</NewProtocol>\
         <NewInternalPort>{}</NewInternalPort>\
         <NewInternalClient>{}</NewInternalClient>\
         <NewEnabled>{}</NewEnabled>\
         <NewPortMappingDescription>{}</NewPortMappingDescription>\
         <NewLeaseDuration>{}</NewLeaseDuration>",
        xml_escape(&m.remote_host),
        m.external_port,
        m.protocol,
        m.internal_port,
        xml_escape(&m.internal_client),
        if m.enabled { "1" } else { "0" },
        xml_escape(&m.description),
        m.remaining_lease(now),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_attr_escape(s: &str) -> String {
    xml_escape(s).replace('"', "&quot;").replace('\'', "&apos;")
}

fn soap_response(service_type: &str, action: &str, body_content: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:{action}Response xmlns:u="{service_type}">
      {body_content}
    </u:{action}Response>
  </s:Body>
</s:Envelope>"#,
        action = action,
        service_type = service_type,
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
