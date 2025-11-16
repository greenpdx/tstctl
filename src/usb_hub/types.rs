use serde::{Deserialize, Serialize};
use std::time::{SystemTime, Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerState {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DeviceRole {
    Master,
    Uut,
    Slave,
}

impl std::fmt::Display for DeviceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceRole::Master => write!(f, "master"),
            DeviceRole::Uut => write!(f, "uut"),
            DeviceRole::Slave => write!(f, "slave"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbHub {
    pub id: String,
    pub vendor_id: String,
    pub product_id: String,
    pub location: String,
    pub name: Option<String>,
    pub ports: Vec<UsbPort>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbPort {
    pub port_number: u8,
    pub power_state: PowerState,
    pub connected_device: Option<String>,
    pub reservation: Option<PortReservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortReservation {
    pub reserved_by: DeviceRole,
    pub reserved_at: SystemTime,
    pub expires_at: Option<SystemTime>,
    pub device_id: Option<String>,
}

impl PortReservation {
    pub fn new(reserved_by: DeviceRole, duration: Option<Duration>) -> Self {
        let reserved_at = SystemTime::now();
        let expires_at = duration.map(|d| reserved_at + d);

        Self {
            reserved_by,
            reserved_at,
            expires_at,
            device_id: None,
        }
    }

    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            SystemTime::now() > expires_at
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortId {
    pub hub_id: String,
    pub port_number: u8,
}

impl std::fmt::Display for HubPortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.hub_id, self.port_number)
    }
}
