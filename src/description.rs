/// Generate the root device description XML.
pub fn device_xml(uuid: &str, host: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:1</deviceType>
    <friendlyName>upnpsim Internet Gateway</friendlyName>
    <manufacturer>upnpsim</manufacturer>
    <modelName>upnpsim</modelName>
    <modelNumber>1.0</modelNumber>
    <UDN>uuid:{uuid}</UDN>
    <deviceList>
      <device>
        <deviceType>urn:schemas-upnp-org:device:WANDevice:1</deviceType>
        <friendlyName>WANDevice</friendlyName>
        <manufacturer>upnpsim</manufacturer>
        <modelName>upnpsim WAN</modelName>
        <UDN>uuid:{uuid}</UDN>
        <deviceList>
          <device>
            <deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:1</deviceType>
            <friendlyName>WANConnectionDevice</friendlyName>
            <manufacturer>upnpsim</manufacturer>
            <modelName>upnpsim WANConn</modelName>
            <UDN>uuid:{uuid}</UDN>
            <serviceList>
              <service>
                <serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType>
                <serviceId>urn:upnp-org:serviceId:WANIPConn1</serviceId>
                <SCPDURL>/scpd/WANIPConn1.xml</SCPDURL>
                <controlURL>/ctl/WANIPConn1</controlURL>
                <eventSubURL>/evt/WANIPConn1</eventSubURL>
              </service>
            </serviceList>
          </device>
        </deviceList>
      </device>
    </deviceList>
  </device>
  <URLBase>http://{host}</URLBase>
</root>"#,
        uuid = uuid,
        host = host,
    )
}

/// Generate a root device description XML that advertises WANIPConnection:2.
pub fn device_v2_xml(uuid: &str, host: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<root xmlns="urn:schemas-upnp-org:device-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <device>
    <deviceType>urn:schemas-upnp-org:device:InternetGatewayDevice:2</deviceType>
    <friendlyName>upnpsim Internet Gateway</friendlyName>
    <manufacturer>upnpsim</manufacturer>
    <modelName>upnpsim</modelName>
    <modelNumber>2.0</modelNumber>
    <UDN>uuid:{uuid}</UDN>
    <deviceList>
      <device>
        <deviceType>urn:schemas-upnp-org:device:WANDevice:2</deviceType>
        <friendlyName>WANDevice</friendlyName>
        <manufacturer>upnpsim</manufacturer>
        <modelName>upnpsim WAN</modelName>
        <UDN>uuid:{uuid}</UDN>
        <deviceList>
          <device>
            <deviceType>urn:schemas-upnp-org:device:WANConnectionDevice:2</deviceType>
            <friendlyName>WANConnectionDevice</friendlyName>
            <manufacturer>upnpsim</manufacturer>
            <modelName>upnpsim WANConn</modelName>
            <UDN>uuid:{uuid}</UDN>
            <serviceList>
              <service>
                <serviceType>urn:schemas-upnp-org:service:WANIPConnection:2</serviceType>
                <serviceId>urn:upnp-org:serviceId:WANIPConn2</serviceId>
                <SCPDURL>/scpd/WANIPConn2.xml</SCPDURL>
                <controlURL>/ctl/WANIPConn2</controlURL>
                <eventSubURL>/evt/WANIPConn2</eventSubURL>
              </service>
            </serviceList>
          </device>
        </deviceList>
      </device>
    </deviceList>
  </device>
  <URLBase>http://{host}</URLBase>
</root>"#,
        uuid = uuid,
        host = host,
    )
}

/// Generate the WANIPConnection service SCPD XML.
pub fn scpd_xml() -> &'static str {
    r#"<?xml version="1.0"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action>
      <name>AddPortMapping</name>
      <argumentList>
        <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
        <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
        <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
        <argument><name>NewInternalPort</name><direction>in</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
        <argument><name>NewInternalClient</name><direction>in</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
        <argument><name>NewEnabled</name><direction>in</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
        <argument><name>NewPortMappingDescription</name><direction>in</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
        <argument><name>NewLeaseDuration</name><direction>in</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
      </argumentList>
    </action>
    <action>
      <name>DeletePortMapping</name>
      <argumentList>
        <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
        <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
        <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      </argumentList>
    </action>
    <action>
      <name>GetExternalIPAddress</name>
      <argumentList>
        <argument><name>NewExternalIPAddress</name><direction>out</direction><relatedStateVariable>ExternalIPAddress</relatedStateVariable></argument>
      </argumentList>
    </action>
    <action>
      <name>GetGenericPortMappingEntry</name>
      <argumentList>
        <argument><name>NewPortMappingIndex</name><direction>in</direction><relatedStateVariable>PortMappingNumberOfEntries</relatedStateVariable></argument>
        <argument><name>NewRemoteHost</name><direction>out</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
        <argument><name>NewExternalPort</name><direction>out</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
        <argument><name>NewProtocol</name><direction>out</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
        <argument><name>NewInternalPort</name><direction>out</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
        <argument><name>NewInternalClient</name><direction>out</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
        <argument><name>NewEnabled</name><direction>out</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
        <argument><name>NewPortMappingDescription</name><direction>out</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
        <argument><name>NewLeaseDuration</name><direction>out</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
      </argumentList>
    </action>
    <action>
      <name>GetSpecificPortMappingEntry</name>
      <argumentList>
        <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
        <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
        <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
        <argument><name>NewInternalPort</name><direction>out</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
        <argument><name>NewInternalClient</name><direction>out</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
        <argument><name>NewEnabled</name><direction>out</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
        <argument><name>NewPortMappingDescription</name><direction>out</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
        <argument><name>NewLeaseDuration</name><direction>out</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
      </argumentList>
    </action>
    <action>
      <name>GetStatusInfo</name>
      <argumentList>
        <argument><name>NewConnectionStatus</name><direction>out</direction><relatedStateVariable>ConnectionStatus</relatedStateVariable></argument>
        <argument><name>NewLastConnectionError</name><direction>out</direction><relatedStateVariable>LastConnectionError</relatedStateVariable></argument>
        <argument><name>NewUptime</name><direction>out</direction><relatedStateVariable>Uptime</relatedStateVariable></argument>
      </argumentList>
    </action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="no"><name>RemoteHost</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>ExternalPort</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingProtocol</name><dataType>string</dataType><allowedValueList><allowedValue>TCP</allowedValue><allowedValue>UDP</allowedValue></allowedValueList></stateVariable>
    <stateVariable sendEvents="no"><name>InternalPort</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>InternalClient</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingEnabled</name><dataType>boolean</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingDescription</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingLeaseDuration</name><dataType>ui4</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingNumberOfEntries</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="yes"><name>ExternalIPAddress</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>ConnectionStatus</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>LastConnectionError</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>Uptime</name><dataType>ui4</dataType></stateVariable>
  </serviceStateTable>
</scpd>"#
}

/// Generate the WANIPConnection:2 service SCPD XML.
pub fn scpd_v2_xml() -> &'static str {
    r#"<?xml version="1.0"?>
<scpd xmlns="urn:schemas-upnp-org:service-1-0">
  <specVersion><major>1</major><minor>0</minor></specVersion>
  <actionList>
    <action><name>SetConnectionType</name><argumentList>
      <argument><name>NewConnectionType</name><direction>in</direction><relatedStateVariable>ConnectionType</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetConnectionTypeInfo</name><argumentList>
      <argument><name>NewConnectionType</name><direction>out</direction><relatedStateVariable>ConnectionType</relatedStateVariable></argument>
      <argument><name>NewPossibleConnectionTypes</name><direction>out</direction><relatedStateVariable>PossibleConnectionTypes</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>RequestConnection</name></action>
    <action><name>ForceTermination</name></action>
    <action><name>GetStatusInfo</name><argumentList>
      <argument><name>NewConnectionStatus</name><direction>out</direction><relatedStateVariable>ConnectionStatus</relatedStateVariable></argument>
      <argument><name>NewLastConnectionError</name><direction>out</direction><relatedStateVariable>LastConnectionError</relatedStateVariable></argument>
      <argument><name>NewUptime</name><direction>out</direction><relatedStateVariable>Uptime</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetNATRSIPStatus</name><argumentList>
      <argument><name>NewRSIPAvailable</name><direction>out</direction><relatedStateVariable>RSIPAvailable</relatedStateVariable></argument>
      <argument><name>NewNATEnabled</name><direction>out</direction><relatedStateVariable>NATEnabled</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>AddPortMapping</name><argumentList>
      <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
      <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewInternalPort</name><direction>in</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
      <argument><name>NewInternalClient</name><direction>in</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
      <argument><name>NewEnabled</name><direction>in</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
      <argument><name>NewPortMappingDescription</name><direction>in</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
      <argument><name>NewLeaseDuration</name><direction>in</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>AddAnyPortMapping</name><argumentList>
      <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
      <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewInternalPort</name><direction>in</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
      <argument><name>NewInternalClient</name><direction>in</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
      <argument><name>NewEnabled</name><direction>in</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
      <argument><name>NewPortMappingDescription</name><direction>in</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
      <argument><name>NewLeaseDuration</name><direction>in</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
      <argument><name>NewReservedPort</name><direction>out</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>DeletePortMapping</name><argumentList>
      <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
      <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>DeletePortMappingRange</name><argumentList>
      <argument><name>NewStartPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewEndPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewManage</name><direction>in</direction><relatedStateVariable>A_ARG_TYPE_Manage</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetExternalIPAddress</name><argumentList>
      <argument><name>NewExternalIPAddress</name><direction>out</direction><relatedStateVariable>ExternalIPAddress</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetGenericPortMappingEntry</name><argumentList>
      <argument><name>NewPortMappingIndex</name><direction>in</direction><relatedStateVariable>PortMappingNumberOfEntries</relatedStateVariable></argument>
      <argument><name>NewRemoteHost</name><direction>out</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
      <argument><name>NewExternalPort</name><direction>out</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>out</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewInternalPort</name><direction>out</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
      <argument><name>NewInternalClient</name><direction>out</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
      <argument><name>NewEnabled</name><direction>out</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
      <argument><name>NewPortMappingDescription</name><direction>out</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
      <argument><name>NewLeaseDuration</name><direction>out</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetSpecificPortMappingEntry</name><argumentList>
      <argument><name>NewRemoteHost</name><direction>in</direction><relatedStateVariable>RemoteHost</relatedStateVariable></argument>
      <argument><name>NewExternalPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewInternalPort</name><direction>out</direction><relatedStateVariable>InternalPort</relatedStateVariable></argument>
      <argument><name>NewInternalClient</name><direction>out</direction><relatedStateVariable>InternalClient</relatedStateVariable></argument>
      <argument><name>NewEnabled</name><direction>out</direction><relatedStateVariable>PortMappingEnabled</relatedStateVariable></argument>
      <argument><name>NewPortMappingDescription</name><direction>out</direction><relatedStateVariable>PortMappingDescription</relatedStateVariable></argument>
      <argument><name>NewLeaseDuration</name><direction>out</direction><relatedStateVariable>PortMappingLeaseDuration</relatedStateVariable></argument>
    </argumentList></action>
    <action><name>GetListOfPortMappings</name><argumentList>
      <argument><name>NewStartPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewEndPort</name><direction>in</direction><relatedStateVariable>ExternalPort</relatedStateVariable></argument>
      <argument><name>NewProtocol</name><direction>in</direction><relatedStateVariable>PortMappingProtocol</relatedStateVariable></argument>
      <argument><name>NewManage</name><direction>in</direction><relatedStateVariable>A_ARG_TYPE_Manage</relatedStateVariable></argument>
      <argument><name>NewNumberOfPorts</name><direction>in</direction><relatedStateVariable>PortMappingNumberOfEntries</relatedStateVariable></argument>
      <argument><name>NewPortListing</name><direction>out</direction><relatedStateVariable>A_ARG_TYPE_PortListing</relatedStateVariable></argument>
    </argumentList></action>
  </actionList>
  <serviceStateTable>
    <stateVariable sendEvents="no"><name>ConnectionType</name><dataType>string</dataType><defaultValue>IP_Routed</defaultValue><allowedValueList><allowedValue>Unconfigured</allowedValue><allowedValue>IP_Routed</allowedValue><allowedValue>IP_Bridged</allowedValue></allowedValueList></stateVariable>
    <stateVariable sendEvents="yes"><name>PossibleConnectionTypes</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="yes"><name>ConnectionStatus</name><dataType>string</dataType><allowedValueList><allowedValue>Unconfigured</allowedValue><allowedValue>Connecting</allowedValue><allowedValue>Connected</allowedValue><allowedValue>PendingDisconnect</allowedValue><allowedValue>Disconnecting</allowedValue><allowedValue>Disconnected</allowedValue></allowedValueList></stateVariable>
    <stateVariable sendEvents="no"><name>Uptime</name><dataType>ui4</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>LastConnectionError</name><dataType>string</dataType><allowedValueList><allowedValue>ERROR_NONE</allowedValue></allowedValueList></stateVariable>
    <stateVariable sendEvents="no"><name>RSIPAvailable</name><dataType>boolean</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>NATEnabled</name><dataType>boolean</dataType></stateVariable>
    <stateVariable sendEvents="yes"><name>ExternalIPAddress</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="yes"><name>PortMappingNumberOfEntries</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingEnabled</name><dataType>boolean</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingLeaseDuration</name><dataType>ui4</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>RemoteHost</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>ExternalPort</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>InternalPort</name><dataType>ui2</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingProtocol</name><dataType>string</dataType><allowedValueList><allowedValue>TCP</allowedValue><allowedValue>UDP</allowedValue></allowedValueList></stateVariable>
    <stateVariable sendEvents="no"><name>InternalClient</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>PortMappingDescription</name><dataType>string</dataType></stateVariable>
    <stateVariable sendEvents="yes"><name>SystemUpdateID</name><dataType>ui4</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>A_ARG_TYPE_Manage</name><dataType>boolean</dataType></stateVariable>
    <stateVariable sendEvents="no"><name>A_ARG_TYPE_PortListing</name><dataType>string</dataType></stateVariable>
  </serviceStateTable>
</scpd>"#
}
