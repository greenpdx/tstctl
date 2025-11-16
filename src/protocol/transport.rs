// Transport layer for test control protocol
//
// Supports:
// - TCP connections (for testing/development)
// - USB gadget networking (for production)

use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};

/// Default TCP port for test control protocol
pub const DEFAULT_TCP_PORT: u16 = 9999;

/// USB gadget network configuration
#[derive(Debug, Clone)]
pub struct UsbGadgetConfig {
    /// Interface name (e.g., "usb0", "usb1")
    pub interface: String,
    /// IP address for this side
    pub ip: String,
    /// Peer IP address
    pub peer_ip: String,
    /// Netmask in CIDR notation (e.g., 24)
    pub netmask: u8,
}

impl Default for UsbGadgetConfig {
    fn default() -> Self {
        Self {
            interface: "usb0".to_string(),
            ip: "192.168.100.1".to_string(),
            peer_ip: "192.168.100.2".to_string(),
            netmask: 24,
        }
    }
}

impl UsbGadgetConfig {
    /// Master configuration (for test controller)
    pub fn master() -> Self {
        Self {
            interface: "usb0".to_string(),
            ip: "192.168.100.1".to_string(),
            peer_ip: "192.168.100.2".to_string(),
            netmask: 24,
        }
    }

    /// Remote configuration (for UUT/slave)
    pub fn remote() -> Self {
        Self {
            interface: "usb0".to_string(),
            ip: "192.168.100.2".to_string(),
            peer_ip: "192.168.100.1".to_string(),
            netmask: 24,
        }
    }
}

/// Transport connection
pub enum Transport {
    /// TCP connection
    Tcp(TcpStream),
}

impl Transport {
    /// Connect to a TCP server
    pub async fn connect_tcp(addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to TCP server")?;
        Ok(Transport::Tcp(stream))
    }

    /// Connect to a remote device via USB gadget network
    pub async fn connect_usb_gadget(config: &UsbGadgetConfig) -> Result<Self> {
        let addr = format!("{}:{}", config.peer_ip, DEFAULT_TCP_PORT);
        let addr: SocketAddr = addr.parse().context("Invalid peer address")?;

        let stream = TcpStream::connect(addr)
            .await
            .context("Failed to connect to USB gadget peer")?;

        Ok(Transport::Tcp(stream))
    }

    /// Split into read and write halves
    pub fn split(self) -> (Box<dyn AsyncRead + Unpin + Send>, Box<dyn AsyncWrite + Unpin + Send>) {
        match self {
            Transport::Tcp(stream) => {
                let (read, write) = stream.into_split();
                (Box::new(read), Box::new(write))
            }
        }
    }
}

/// Transport server (listens for connections)
pub enum TransportServer {
    /// TCP server
    Tcp(TcpListener),
}

impl TransportServer {
    /// Create a TCP server
    pub async fn bind_tcp(addr: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(addr)
            .await
            .context("Failed to bind TCP server")?;
        Ok(TransportServer::Tcp(listener))
    }

    /// Create a USB gadget server
    pub async fn bind_usb_gadget(config: &UsbGadgetConfig) -> Result<Self> {
        let addr = format!("{}:{}", config.ip, DEFAULT_TCP_PORT);
        let addr: SocketAddr = addr.parse().context("Invalid bind address")?;

        let listener = TcpListener::bind(addr)
            .await
            .context("Failed to bind USB gadget server")?;

        Ok(TransportServer::Tcp(listener))
    }

    /// Accept a connection
    pub async fn accept(&self) -> Result<(Transport, SocketAddr)> {
        match self {
            TransportServer::Tcp(listener) => {
                let (stream, addr) = listener
                    .accept()
                    .await
                    .context("Failed to accept connection")?;
                Ok((Transport::Tcp(stream), addr))
            }
        }
    }
}

/// Setup USB gadget networking on Linux
pub fn setup_usb_gadget(config: &UsbGadgetConfig) -> Result<()> {
    use std::process::Command;

    // This requires root privileges and configfs support

    // 1. Configure IP address
    let status = Command::new("ip")
        .args([
            "addr",
            "add",
            &format!("{}/{}", config.ip, config.netmask),
            "dev",
            &config.interface,
        ])
        .status()
        .context("Failed to configure IP address")?;

    if !status.success() {
        // Ignore error if address already exists
        tracing::warn!("IP address configuration returned non-zero status");
    }

    // 2. Bring interface up
    let status = Command::new("ip")
        .args(["link", "set", &config.interface, "up"])
        .status()
        .context("Failed to bring interface up")?;

    if !status.success() {
        anyhow::bail!("Failed to bring interface up");
    }

    tracing::info!(
        "USB gadget networking configured on {} with IP {}",
        config.interface,
        config.ip
    );

    Ok(())
}

/// Setup USB gadget device (configfs)
/// This creates the USB gadget device that will appear as a network interface
pub fn setup_usb_gadget_device() -> Result<()> {
    use std::fs;
    use std::path::Path;

    let configfs = Path::new("/sys/kernel/config/usb_gadget");
    if !configfs.exists() {
        // Try to mount configfs
        std::process::Command::new("mount")
            .args(["-t", "configfs", "none", "/sys/kernel/config"])
            .status()
            .context("Failed to mount configfs")?;
    }

    let gadget_path = configfs.join("testctl");
    if gadget_path.exists() {
        tracing::info!("USB gadget already configured");
        return Ok(());
    }

    // Create gadget directory
    fs::create_dir(&gadget_path).context("Failed to create gadget directory")?;

    // Set USB IDs (vendor/product)
    fs::write(gadget_path.join("idVendor"), "0x1d6b")
        .context("Failed to set vendor ID")?;
    fs::write(gadget_path.join("idProduct"), "0x0104")
        .context("Failed to set product ID")?;

    // Set device info
    let strings = gadget_path.join("strings/0x409");
    fs::create_dir_all(&strings).context("Failed to create strings directory")?;
    fs::write(strings.join("serialnumber"), "testctl-001")
        .context("Failed to set serial number")?;
    fs::write(strings.join("manufacturer"), "TestCtl")
        .context("Failed to set manufacturer")?;
    fs::write(strings.join("product"), "TestCtl Network")
        .context("Failed to set product")?;

    // Create configuration
    let config = gadget_path.join("configs/c.1");
    fs::create_dir_all(&config).context("Failed to create config directory")?;

    let config_strings = config.join("strings/0x409");
    fs::create_dir_all(&config_strings).context("Failed to create config strings directory")?;
    fs::write(config_strings.join("configuration"), "RNDIS")
        .context("Failed to set configuration")?;

    // Create RNDIS function (USB networking)
    let function = gadget_path.join("functions/rndis.usb0");
    fs::create_dir_all(&function).context("Failed to create function directory")?;

    // Link function to configuration
    let link_target = config.join("rndis.usb0");
    if !link_target.exists() {
        std::os::unix::fs::symlink(&function, &link_target)
            .context("Failed to link function to configuration")?;
    }

    // Enable the gadget by binding to UDC
    let udc_path = Path::new("/sys/class/udc");
    if let Ok(entries) = fs::read_dir(udc_path) {
        if let Some(Ok(entry)) = entries.into_iter().next() {
            let udc_name = entry.file_name();
            fs::write(gadget_path.join("UDC"), udc_name.to_str().unwrap())
                .context("Failed to bind to UDC")?;
            tracing::info!("USB gadget device enabled");
        } else {
            tracing::warn!("No UDC found - USB gadget may not work");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tcp_server_client() {
        let addr = "127.0.0.1:0".parse().unwrap();
        let server = TransportServer::bind_tcp(addr).await.unwrap();

        let server_addr = match &server {
            TransportServer::Tcp(listener) => listener.local_addr().unwrap(),
        };

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let (_transport, _addr) = server.accept().await.unwrap();
        });

        // Connect client
        let _client = Transport::connect_tcp(server_addr).await.unwrap();

        // Wait for server to accept
        server_task.await.unwrap();
    }
}
