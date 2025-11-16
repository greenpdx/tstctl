use super::types::{UsbHub, UsbPort, PowerState};
use std::process::Command;

#[derive(Debug)]
pub enum UhubctlError {
    CommandFailed(String),
    ParseError(String),
    NotFound(String),
}

impl std::fmt::Display for UhubctlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UhubctlError::CommandFailed(msg) => write!(f, "Command failed: {}", msg),
            UhubctlError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            UhubctlError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl std::error::Error for UhubctlError {}

pub type Result<T> = std::result::Result<T, UhubctlError>;

/// Wrapper for uhubctl command-line tool
pub struct Uhubctl {
    uhubctl_path: String,
}

impl Default for Uhubctl {
    fn default() -> Self {
        Self {
            uhubctl_path: "uhubctl".to_string(),
        }
    }
}

impl Uhubctl {
    pub fn new(uhubctl_path: Option<String>) -> Self {
        Self {
            uhubctl_path: uhubctl_path.unwrap_or_else(|| "uhubctl".to_string()),
        }
    }

    /// List all USB hubs
    pub fn list_hubs(&self) -> Result<Vec<UsbHub>> {
        let output = Command::new(&self.uhubctl_path)
            .output()
            .map_err(|e| UhubctlError::CommandFailed(format!("Failed to execute uhubctl: {}", e)))?;

        if !output.status.success() {
            return Err(UhubctlError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        self.parse_hub_list(&stdout)
    }

    /// Power on a specific port
    pub fn power_on(&self, location: &str, port: u8) -> Result<()> {
        self.set_power(location, port, true)
    }

    /// Power off a specific port
    pub fn power_off(&self, location: &str, port: u8) -> Result<()> {
        self.set_power(location, port, false)
    }

    /// Set power state for a port
    fn set_power(&self, location: &str, port: u8, on: bool) -> Result<()> {
        let action = if on { "1" } else { "0" };

        let output = Command::new(&self.uhubctl_path)
            .args(&[
                "-l", location,
                "-p", &port.to_string(),
                "-a", action,
            ])
            .output()
            .map_err(|e| UhubctlError::CommandFailed(format!("Failed to execute uhubctl: {}", e)))?;

        if !output.status.success() {
            return Err(UhubctlError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Ok(())
    }

    /// Get status of a specific hub
    pub fn get_hub_status(&self, location: &str) -> Result<UsbHub> {
        let output = Command::new(&self.uhubctl_path)
            .args(&["-l", location])
            .output()
            .map_err(|e| UhubctlError::CommandFailed(format!("Failed to execute uhubctl: {}", e)))?;

        if !output.status.success() {
            return Err(UhubctlError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let hubs = self.parse_hub_list(&stdout)?;

        hubs.into_iter()
            .find(|h| h.location == location)
            .ok_or_else(|| UhubctlError::NotFound(format!("Hub not found at location: {}", location)))
    }

    /// Parse uhubctl output to extract hub information
    fn parse_hub_list(&self, output: &str) -> Result<Vec<UsbHub>> {
        let mut hubs = Vec::new();
        let mut current_hub: Option<UsbHub> = None;
        let mut current_ports: Vec<UsbPort> = Vec::new();

        for line in output.lines() {
            let line = line.trim();

            // Parse hub header: "Current status for hub 1-1.4 [2109:2817 USB2.0 Hub, USB 2.00, 4 ports]"
            if line.starts_with("Current status for hub") {
                // Save previous hub if exists
                if let Some(mut hub) = current_hub.take() {
                    hub.ports = std::mem::take(&mut current_ports);
                    hubs.push(hub);
                }

                // Parse new hub
                if let Some(location) = self.extract_between(line, "hub ", " [") {
                    if let Some(ids) = self.extract_between(line, "[", " ") {
                        let parts: Vec<&str> = ids.split(':').collect();
                        if parts.len() == 2 {
                            let vendor_id = parts[0].to_string();
                            let product_id = parts[1].to_string();
                            let id = format!("{}:{}", vendor_id, product_id);

                            current_hub = Some(UsbHub {
                                id: id.clone(),
                                vendor_id,
                                product_id,
                                location: location.to_string(),
                                name: None,
                                ports: Vec::new(),
                            });
                        }
                    }
                }
            }
            // Parse port status: "   Port 1: 0100 power"
            else if line.contains("Port ") {
                if let Some(port_num_str) = self.extract_between(line, "Port ", ":") {
                    if let Ok(port_number) = port_num_str.trim().parse::<u8>() {
                        let power_state = if line.contains("power") {
                            PowerState::On
                        } else {
                            PowerState::Off
                        };

                        current_ports.push(UsbPort {
                            port_number,
                            power_state,
                            connected_device: None,
                            reservation: None,
                        });
                    }
                }
            }
        }

        // Save last hub
        if let Some(mut hub) = current_hub.take() {
            hub.ports = current_ports;
            hubs.push(hub);
        }

        Ok(hubs)
    }

    fn extract_between<'a>(&self, text: &'a str, start: &str, end: &str) -> Option<&'a str> {
        let start_idx = text.find(start)? + start.len();
        let remaining = &text[start_idx..];
        let end_idx = remaining.find(end)?;
        Some(&remaining[..end_idx])
    }

    /// Check if uhubctl is available on the system
    pub fn is_available(&self) -> bool {
        Command::new(&self.uhubctl_path)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uhubctl_output() {
        let uhubctl = Uhubctl::default();
        let output = r#"
Current status for hub 1-1.4 [2109:2817 USB2.0 Hub, USB 2.00, 4 ports]
  Port 1: 0100 power
  Port 2: 0100 power
  Port 3: 0503 power highspeed enable connect [05e3:0749 USB2.0 Hub]
  Port 4: 0100 power
        "#;

        let hubs = uhubctl.parse_hub_list(output).unwrap();
        assert_eq!(hubs.len(), 1);

        let hub = &hubs[0];
        assert_eq!(hub.location, "1-1.4");
        assert_eq!(hub.vendor_id, "2109");
        assert_eq!(hub.product_id, "2817");
        assert_eq!(hub.ports.len(), 4);

        assert_eq!(hub.ports[0].port_number, 1);
        assert_eq!(hub.ports[0].power_state, PowerState::On);

        assert_eq!(hub.ports[2].port_number, 3);
        assert_eq!(hub.ports[2].power_state, PowerState::On);
    }
}
