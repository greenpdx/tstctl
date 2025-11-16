use super::types::{UsbHub, UsbPort, PowerState, DeviceRole, PortReservation};
use super::uhubctl::{Uhubctl, UhubctlError};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Debug)]
pub enum ManagerError {
    UhubctlError(UhubctlError),
    HubNotFound(String),
    PortNotFound(u8),
    PortAlreadyReserved(DeviceRole),
    ReservationNotFound,
    InvalidOperation(String),
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::UhubctlError(e) => write!(f, "Uhubctl error: {}", e),
            ManagerError::HubNotFound(id) => write!(f, "Hub not found: {}", id),
            ManagerError::PortNotFound(port) => write!(f, "Port not found: {}", port),
            ManagerError::PortAlreadyReserved(by) => write!(f, "Port already reserved by: {}", by),
            ManagerError::ReservationNotFound => write!(f, "Reservation not found"),
            ManagerError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
        }
    }
}

impl std::error::Error for ManagerError {}

impl From<UhubctlError> for ManagerError {
    fn from(error: UhubctlError) -> Self {
        ManagerError::UhubctlError(error)
    }
}

pub type Result<T> = std::result::Result<T, ManagerError>;

/// USB Hub Manager - manages hub state and port reservations
pub struct UsbHubManager {
    uhubctl: Uhubctl,
    hubs: Arc<RwLock<HashMap<String, UsbHub>>>,
}

impl UsbHubManager {
    pub fn new(uhubctl_path: Option<String>) -> Self {
        Self {
            uhubctl: Uhubctl::new(uhubctl_path),
            hubs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Discover and register all USB hubs
    pub async fn discover_hubs(&self) -> Result<Vec<UsbHub>> {
        let discovered_hubs = self.uhubctl.list_hubs()?;

        let mut hubs = self.hubs.write().await;
        for hub in &discovered_hubs {
            hubs.insert(hub.id.clone(), hub.clone());
        }

        Ok(discovered_hubs)
    }

    /// Get list of all registered hubs
    pub async fn list_hubs(&self) -> Vec<UsbHub> {
        let hubs = self.hubs.read().await;
        hubs.values().cloned().collect()
    }

    /// Get a specific hub by ID
    pub async fn get_hub(&self, hub_id: &str) -> Result<UsbHub> {
        let hubs = self.hubs.read().await;
        hubs.get(hub_id)
            .cloned()
            .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))
    }

    /// Update hub status from hardware
    pub async fn refresh_hub(&self, hub_id: &str) -> Result<UsbHub> {
        let location = {
            let hubs = self.hubs.read().await;
            let hub = hubs.get(hub_id)
                .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))?;
            hub.location.clone()
        };

        let updated_hub = self.uhubctl.get_hub_status(&location)?;

        // Preserve reservations from old state
        let mut hubs = self.hubs.write().await;
        if let Some(old_hub) = hubs.get(hub_id) {
            let mut merged_hub = updated_hub.clone();
            for port in &mut merged_hub.ports {
                if let Some(old_port) = old_hub.ports.iter().find(|p| p.port_number == port.port_number) {
                    // Preserve reservation if not expired
                    if let Some(reservation) = &old_port.reservation {
                        if !reservation.is_expired() {
                            port.reservation = Some(reservation.clone());
                        }
                    }
                }
            }
            hubs.insert(hub_id.to_string(), merged_hub.clone());
            Ok(merged_hub)
        } else {
            hubs.insert(hub_id.to_string(), updated_hub.clone());
            Ok(updated_hub)
        }
    }

    /// Reserve a port for exclusive access
    pub async fn reserve_port(
        &self,
        hub_id: &str,
        port_number: u8,
        reserved_by: DeviceRole,
        duration: Option<Duration>,
    ) -> Result<()> {
        let mut hubs = self.hubs.write().await;
        let hub = hubs.get_mut(hub_id)
            .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))?;

        let port = hub.ports.iter_mut()
            .find(|p| p.port_number == port_number)
            .ok_or(ManagerError::PortNotFound(port_number))?;

        // Check if already reserved
        if let Some(existing_reservation) = &port.reservation {
            if !existing_reservation.is_expired() {
                return Err(ManagerError::PortAlreadyReserved(existing_reservation.reserved_by));
            }
        }

        // Create new reservation
        port.reservation = Some(PortReservation::new(reserved_by, duration));

        Ok(())
    }

    /// Release a port reservation
    pub async fn release_port(
        &self,
        hub_id: &str,
        port_number: u8,
        released_by: DeviceRole,
    ) -> Result<()> {
        let mut hubs = self.hubs.write().await;
        let hub = hubs.get_mut(hub_id)
            .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))?;

        let port = hub.ports.iter_mut()
            .find(|p| p.port_number == port_number)
            .ok_or(ManagerError::PortNotFound(port_number))?;

        // Verify the reservation belongs to the requester
        if let Some(reservation) = &port.reservation {
            if reservation.reserved_by != released_by {
                return Err(ManagerError::InvalidOperation(
                    format!("Port is reserved by {}, cannot be released by {}",
                            reservation.reserved_by, released_by)
                ));
            }
        } else {
            return Err(ManagerError::ReservationNotFound);
        }

        port.reservation = None;

        Ok(())
    }

    /// Power on a port
    pub async fn power_on_port(&self, hub_id: &str, port_number: u8) -> Result<()> {
        let location = {
            let hubs = self.hubs.read().await;
            let hub = hubs.get(hub_id)
                .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))?;
            hub.location.clone()
        };

        self.uhubctl.power_on(&location, port_number)?;

        // Update state
        let mut hubs = self.hubs.write().await;
        if let Some(hub) = hubs.get_mut(hub_id) {
            if let Some(port) = hub.ports.iter_mut().find(|p| p.port_number == port_number) {
                port.power_state = PowerState::On;
            }
        }

        Ok(())
    }

    /// Power off a port
    pub async fn power_off_port(&self, hub_id: &str, port_number: u8) -> Result<()> {
        let location = {
            let hubs = self.hubs.read().await;
            let hub = hubs.get(hub_id)
                .ok_or_else(|| ManagerError::HubNotFound(hub_id.to_string()))?;
            hub.location.clone()
        };

        self.uhubctl.power_off(&location, port_number)?;

        // Update state
        let mut hubs = self.hubs.write().await;
        if let Some(hub) = hubs.get_mut(hub_id) {
            if let Some(port) = hub.ports.iter_mut().find(|p| p.port_number == port_number) {
                port.power_state = PowerState::Off;
            }
        }

        Ok(())
    }

    /// Reserve and power on a port (atomic operation)
    pub async fn reserve_and_power_on(
        &self,
        hub_id: &str,
        port_number: u8,
        reserved_by: DeviceRole,
        duration: Option<Duration>,
    ) -> Result<()> {
        // Reserve first
        self.reserve_port(hub_id, port_number, reserved_by, duration).await?;

        // Then power on
        if let Err(e) = self.power_on_port(hub_id, port_number).await {
            // Rollback reservation on failure
            let _ = self.release_port(hub_id, port_number, reserved_by).await;
            return Err(e);
        }

        Ok(())
    }

    /// Power off and release a port (atomic operation)
    pub async fn power_off_and_release(
        &self,
        hub_id: &str,
        port_number: u8,
        released_by: DeviceRole,
    ) -> Result<()> {
        // Power off first
        self.power_off_port(hub_id, port_number).await?;

        // Then release
        self.release_port(hub_id, port_number, released_by).await?;

        Ok(())
    }

    /// Clean up expired reservations
    pub async fn cleanup_expired_reservations(&self) -> usize {
        let mut hubs = self.hubs.write().await;
        let mut count = 0;

        for hub in hubs.values_mut() {
            for port in &mut hub.ports {
                if let Some(reservation) = &port.reservation {
                    if reservation.is_expired() {
                        port.reservation = None;
                        count += 1;
                    }
                }
            }
        }

        count
    }

    /// Check if uhubctl is available
    pub fn is_uhubctl_available(&self) -> bool {
        self.uhubctl.is_available()
    }

    /// Get list of ports reserved by a specific role
    pub async fn get_reserved_ports(&self, role: DeviceRole) -> Vec<(String, u8)> {
        let hubs = self.hubs.read().await;
        let mut reserved_ports = Vec::new();

        for (hub_id, hub) in hubs.iter() {
            for port in &hub.ports {
                if let Some(reservation) = &port.reservation {
                    if reservation.reserved_by == role && !reservation.is_expired() {
                        reserved_ports.push((hub_id.clone(), port.port_number));
                    }
                }
            }
        }

        reserved_ports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_port_reservation() {
        let manager = UsbHubManager::new(None);

        // Manually add a test hub
        let test_hub = UsbHub {
            id: "test:hub".to_string(),
            vendor_id: "1234".to_string(),
            product_id: "5678".to_string(),
            location: "1-1".to_string(),
            name: Some("Test Hub".to_string()),
            ports: vec![
                UsbPort {
                    port_number: 1,
                    power_state: PowerState::Off,
                    connected_device: None,
                    reservation: None,
                },
            ],
        };

        {
            let mut hubs = manager.hubs.write().await;
            hubs.insert(test_hub.id.clone(), test_hub);
        }

        // Test reservation
        let result = manager.reserve_port("test:hub", 1, DeviceRole::Master, None).await;
        assert!(result.is_ok());

        // Test double reservation
        let result = manager.reserve_port("test:hub", 1, DeviceRole::Uut, None).await;
        assert!(matches!(result, Err(ManagerError::PortAlreadyReserved(_))));

        // Test release
        let result = manager.release_port("test:hub", 1, DeviceRole::Master).await;
        assert!(result.is_ok());

        // Test release when not reserved
        let result = manager.release_port("test:hub", 1, DeviceRole::Master).await;
        assert!(matches!(result, Err(ManagerError::ReservationNotFound)));
    }
}
