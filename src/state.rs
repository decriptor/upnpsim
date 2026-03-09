use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Shared state handle — clone-friendly.
pub type SharedState = Arc<RwLock<SimState>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TtlMode {
    Respect,
    Disrespect,
}

impl std::fmt::Display for TtlMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtlMode::Respect => write!(f, "respect"),
            TtlMode::Disrespect => write!(f, "disrespect"),
        }
    }
}

impl std::str::FromStr for TtlMode {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "respect" => Ok(TtlMode::Respect),
            "disrespect" => Ok(TtlMode::Disrespect),
            _ => Err(anyhow::anyhow!("invalid TTL mode: {s}, expected 'respect' or 'disrespect'")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MappingKey {
    pub protocol: String,
    pub external_port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortMapping {
    pub enabled: bool,
    pub internal_client: String,
    pub internal_port: u16,
    pub external_port: u16,
    pub protocol: String,
    pub description: String,
    /// Lease duration in seconds. 0 means permanent.
    pub lease_duration: u32,
    /// Virtual-clock seconds when this mapping was created.
    pub created_at_virtual_secs: i64,
}

impl PortMapping {
    /// Returns true if this mapping has expired given the current virtual time.
    pub fn is_expired(&self, now_virtual_secs: i64) -> bool {
        if self.lease_duration == 0 {
            return false;
        }
        let expiry = self.created_at_virtual_secs + self.lease_duration as i64;
        now_virtual_secs >= expiry
    }

    pub fn remaining_lease(&self, now_virtual_secs: i64) -> u32 {
        if self.lease_duration == 0 {
            return 0;
        }
        let expiry = self.created_at_virtual_secs + self.lease_duration as i64;
        let remaining = expiry - now_virtual_secs;
        if remaining < 0 { 0 } else { remaining as u32 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenaSubscription {
    pub sid: String,
    pub callback_url: String,
    pub timeout_secs: u32,
    pub created_at_virtual_secs: i64,
}

impl GenaSubscription {
    pub fn is_expired(&self, now_virtual_secs: i64) -> bool {
        if self.timeout_secs == 0 {
            return false;
        }
        let expiry = self.created_at_virtual_secs + self.timeout_secs as i64;
        now_virtual_secs >= expiry
    }
}

#[derive(Debug)]
pub struct SimState {
    pub mappings: HashMap<MappingKey, PortMapping>,
    pub mapping_order: Vec<MappingKey>,
    pub ttl_mode: TtlMode,
    pub external_ip: String,
    pub subscriptions: HashMap<String, GenaSubscription>,
    pub device_uuid: String,
}

impl SimState {
    pub fn new(external_ip: String) -> Self {
        Self {
            mappings: HashMap::new(),
            mapping_order: Vec::new(),
            ttl_mode: TtlMode::Respect,
            external_ip,
            subscriptions: HashMap::new(),
            device_uuid: uuid::Uuid::new_v4().to_string(),
        }
    }

    pub fn add_mapping(&mut self, mapping: PortMapping) -> Result<(), UPnPError> {
        let key = MappingKey {
            protocol: mapping.protocol.clone(),
            external_port: mapping.external_port,
        };
        if self.mappings.contains_key(&key) {
            return Err(UPnPError::ConflictInMappingEntry);
        }
        self.mapping_order.push(key.clone());
        self.mappings.insert(key, mapping);
        Ok(())
    }

    pub fn delete_mapping(&mut self, protocol: &str, external_port: u16) -> Result<(), UPnPError> {
        let key = MappingKey {
            protocol: protocol.to_string(),
            external_port,
        };
        if self.mappings.remove(&key).is_none() {
            return Err(UPnPError::NoSuchEntryInArray);
        }
        self.mapping_order.retain(|k| k != &key);
        Ok(())
    }

    pub fn get_mapping_by_index(&self, index: usize) -> Result<&PortMapping, UPnPError> {
        self.mapping_order
            .get(index)
            .and_then(|k| self.mappings.get(k))
            .ok_or(UPnPError::NoSuchEntryInArray)
    }

    pub fn get_mapping(&self, protocol: &str, external_port: u16) -> Result<&PortMapping, UPnPError> {
        let key = MappingKey {
            protocol: protocol.to_string(),
            external_port,
        };
        self.mappings.get(&key).ok_or(UPnPError::NoSuchEntryInArray)
    }

    /// Remove all expired mappings. Returns number removed.
    pub fn reap_expired(&mut self, now_virtual_secs: i64) -> usize {
        let expired_keys: Vec<MappingKey> = self
            .mappings
            .iter()
            .filter(|(_, m)| m.is_expired(now_virtual_secs))
            .map(|(k, _)| k.clone())
            .collect();
        let count = expired_keys.len();
        for key in &expired_keys {
            self.mappings.remove(key);
        }
        self.mapping_order.retain(|k| !expired_keys.contains(k));
        count
    }
}

#[derive(Debug, Clone)]
pub enum UPnPError {
    NoSuchEntryInArray,
    ConflictInMappingEntry,
    InvalidArgs,
    ActionFailed,
}

impl UPnPError {
    pub fn code(&self) -> u16 {
        match self {
            UPnPError::NoSuchEntryInArray => 714,
            UPnPError::ConflictInMappingEntry => 718,
            UPnPError::InvalidArgs => 402,
            UPnPError::ActionFailed => 501,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            UPnPError::NoSuchEntryInArray => "NoSuchEntryInArray",
            UPnPError::ConflictInMappingEntry => "ConflictInMappingEntry",
            UPnPError::InvalidArgs => "InvalidArgs",
            UPnPError::ActionFailed => "ActionFailed",
        }
    }
}

impl std::fmt::Display for UPnPError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "UPnPError {}: {}", self.code(), self.description())
    }
}

impl std::error::Error for UPnPError {}
