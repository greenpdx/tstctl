// Test Control Protocol - Remote (UUT/Slave)
//
// Runs on the device under test or slave devices.
// Receives commands from master and executes tests.

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{BufReader, BufWriter};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use testctl::{
    read_message, write_message, DeviceInfo, DeviceRole, Message, Method, NetworkConfigureParams,
    NetworkStatusData, SetRoleParams, TestAbortParams,
    TestResultsData, TestResultsParams, TestStartParams, TestStartResult, TestStatus,
    TransportServer, UsbGadgetConfig, error_codes, setup_usb_gadget, setup_usb_gadget_device,
    UsbHubManager, HubDeviceRole, HubInfo, HubPortInfo, HubPortPowerState,
    HubPortReservationInfo, HubListResult, HubDiscoverResult, HubStatusParams,
    HubPortPowerOnParams, HubPortPowerOffParams, HubPortReserveParams, HubPortReleaseParams,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Test Control Protocol Remote", long_about = None)]
struct Args {
    /// Device role (uut or slave)
    #[arg(short, long, default_value = "uut")]
    role: String,

    /// USB gadget interface (e.g., usb0)
    #[arg(short = 'i', long, default_value = "usb0")]
    interface: String,

    /// Use TCP instead of USB gadget (for testing)
    #[arg(long)]
    tcp: bool,

    /// TCP listen address (if --tcp is used)
    #[arg(long, default_value = "0.0.0.0:9999")]
    tcp_addr: String,

    /// Setup USB gadget device before starting
    #[arg(long)]
    setup_gadget: bool,
}

/// Remote device state
struct RemoteState {
    /// Device role
    role: DeviceRole,
    /// Running tests
    tests: HashMap<String, TestResultsData>,
    /// Test counter for generating IDs
    test_counter: u64,
    /// USB hub manager
    hub_manager: Arc<UsbHubManager>,
}

impl RemoteState {
    fn new(role: DeviceRole) -> Self {
        Self {
            role,
            tests: HashMap::new(),
            test_counter: 0,
            hub_manager: Arc::new(UsbHubManager::new(None)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // Parse role
    let role = match args.role.to_lowercase().as_str() {
        "uut" => DeviceRole::Uut,
        "slave" => DeviceRole::Slave,
        _ => {
            error!("Invalid role: {}. Must be 'uut' or 'slave'", args.role);
            std::process::exit(1);
        }
    };

    info!("Starting testctl-remote as {:?}", role);

    // Setup USB gadget if requested
    if args.setup_gadget && !args.tcp {
        info!("Setting up USB gadget device...");
        if let Err(e) = setup_usb_gadget_device() {
            error!("Failed to setup USB gadget device: {}", e);
            warn!("Continuing anyway - device may already be configured");
        }

        let config = UsbGadgetConfig::remote();
        info!("Configuring USB gadget network on {}", config.interface);
        if let Err(e) = setup_usb_gadget(&config) {
            error!("Failed to configure USB gadget network: {}", e);
            std::process::exit(1);
        }
    }

    // Create transport server
    let server = if args.tcp {
        let addr: SocketAddr = args
            .tcp_addr
            .parse()
            .context("Invalid TCP listen address")?;
        info!("Starting TCP server on {}", addr);
        TransportServer::bind_tcp(addr).await?
    } else {
        let config = UsbGadgetConfig::remote();
        info!("Starting USB gadget server on {}", config.ip);
        TransportServer::bind_usb_gadget(&config).await?
    };

    // Create shared state
    let state = Arc::new(RwLock::new(RemoteState::new(role)));

    info!("Remote ready, waiting for connections...");

    // Accept connections
    loop {
        match server.accept().await {
            Ok((transport, addr)) => {
                info!("New connection from {}", addr);
                let state = Arc::clone(&state);

                tokio::spawn(async move {
                    if let Err(e) = handle_connection(transport, state).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}

/// Handle a single connection
async fn handle_connection(
    transport: testctl::Transport,
    state: Arc<RwLock<RemoteState>>,
) -> Result<()> {
    let (read, write) = transport.split();
    let mut reader = BufReader::new(read);
    let mut writer = BufWriter::new(write);

    loop {
        // Read message
        let message = match read_message(&mut reader).await {
            Ok(msg) => msg,
            Err(e) => {
                if e.to_string().contains("unexpected end of file") {
                    info!("Connection closed");
                    break;
                }
                error!("Failed to read message: {}", e);
                break;
            }
        };

        // Handle message
        match message {
            Message::Request(req) => {
                let response = handle_request(req, &state).await;
                if let Err(e) = write_message(&mut writer, &response).await {
                    error!("Failed to write response: {}", e);
                    break;
                }
            }
            Message::Event(_) => {
                warn!("Received event on remote (events should only be sent by remote)");
            }
            Message::Response(_) => {
                warn!("Received response on remote (responses should only be sent by remote)");
            }
        }
    }

    Ok(())
}

/// Handle a request
async fn handle_request(
    req: testctl::Request,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    let id = req.id.clone();

    info!("Handling request: {:?}", req.method);

    match req.method {
        Method::Ping => handle_ping(id),
        Method::GetInfo => handle_get_info(id, state).await,
        Method::SetRole => handle_set_role(id, req.params, state).await,
        Method::TestStart => handle_test_start(id, req.params, state).await,
        Method::TestAbort => handle_test_abort(id, req.params, state).await,
        Method::TestCancel => handle_test_cancel(id, req.params, state).await,
        Method::TestResults => handle_test_results(id, req.params, state).await,
        Method::TestStatus => handle_test_status(id, req.params, state).await,
        Method::NetworkConfigure => handle_network_configure(id, req.params).await,
        Method::NetworkStatus => handle_network_status(id, req.params).await,

        // USB Hub methods
        Method::HubList => handle_hub_list(id, state).await,
        Method::HubDiscover => handle_hub_discover(id, state).await,
        Method::HubStatus => handle_hub_status(id, req.params, state).await,
        Method::HubPortPowerOn => handle_hub_port_power_on(id, req.params, state).await,
        Method::HubPortPowerOff => handle_hub_port_power_off(id, req.params, state).await,
        Method::HubPortReserve => handle_hub_port_reserve(id, req.params, state).await,
        Method::HubPortRelease => handle_hub_port_release(id, req.params, state).await,
    }
}

/// Handle ping request
fn handle_ping(id: String) -> Message {
    Message::success(id, serde_json::json!({"pong": true}))
}

/// Handle get_info request
async fn handle_get_info(id: String, state: &Arc<RwLock<RemoteState>>) -> Message {
    let state = state.read().await;

    let hostname = hostname::get()
        .unwrap_or_else(|_| std::ffi::OsString::from("unknown"))
        .to_string_lossy()
        .to_string();

    let info = DeviceInfo {
        hostname,
        role: state.role,
        protocol_version: testctl::protocol::PROTOCOL_VERSION.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime: get_uptime(),
        test_suites: vec!["api_tests".to_string(), "integration_tests".to_string()],
    };

    Message::success(id, serde_json::to_value(info).unwrap())
}

/// Handle set_role request
async fn handle_set_role(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<SetRoleParams>(params) {
        Ok(p) => {
            let mut state = state.write().await;
            state.role = p.role;
            info!("Role changed to {:?}", p.role);
            Message::success(id, serde_json::json!({"role": p.role}))
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid params: {}", e),
        ),
    }
}

/// Handle test_start request
async fn handle_test_start(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<TestStartParams>(params) {
        Ok(p) => {
            let mut state = state.write().await;

            // Generate test ID
            state.test_counter += 1;
            let test_id = format!("test-{}", state.test_counter);

            // Create test record
            let test = TestResultsData {
                test_id: test_id.clone(),
                status: TestStatus::Running,
                suite: p.suite.clone(),
                test_case: p.test_case.clone(),
                start_time: chrono::Utc::now(),
                end_time: None,
                duration: None,
                passed: 0,
                failed: 0,
                error: None,
                logs: Some(vec![]),
            };

            state.tests.insert(test_id.clone(), test);

            info!("Started test: {} (suite: {})", test_id, p.suite);

            // TODO: Actually run the test asynchronously

            let result = TestStartResult {
                test_id,
                status: TestStatus::Running,
            };

            Message::success(id, serde_json::to_value(result).unwrap())
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid params: {}", e),
        ),
    }
}

/// Handle test_abort request
async fn handle_test_abort(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<TestAbortParams>(params) {
        Ok(p) => {
            let mut state = state.write().await;

            if let Some(test) = state.tests.get_mut(&p.test_id) {
                test.status = TestStatus::Aborted;
                test.end_time = Some(chrono::Utc::now());
                test.error = p.reason;

                info!("Aborted test: {}", p.test_id);

                Message::success(id, serde_json::json!({"aborted": true}))
            } else {
                Message::error(
                    id,
                    error_codes::TEST_NOT_FOUND,
                    format!("Test not found: {}", p.test_id),
                )
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid params: {}", e),
        ),
    }
}

/// Handle test_cancel request
async fn handle_test_cancel(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    // Similar to abort but gentler
    handle_test_abort(id, params, state).await
}

/// Handle test_results request
async fn handle_test_results(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<TestResultsParams>(params) {
        Ok(p) => {
            let state = state.read().await;

            if let Some(test) = state.tests.get(&p.test_id) {
                let mut result = test.clone();

                // Remove logs if not requested
                if !p.include_logs {
                    result.logs = None;
                }

                Message::success(id, serde_json::to_value(result).unwrap())
            } else {
                Message::error(
                    id,
                    error_codes::TEST_NOT_FOUND,
                    format!("Test not found: {}", p.test_id),
                )
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid params: {}", e),
        ),
    }
}

/// Handle test_status request
async fn handle_test_status(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    // Reuse test_results but without logs
    let mut params_obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_value(params).unwrap_or_default();
    params_obj.insert("include_logs".to_string(), serde_json::json!(false));

    handle_test_results(id, serde_json::Value::Object(params_obj), state).await
}

/// Handle network_configure request
async fn handle_network_configure(id: String, params: serde_json::Value) -> Message {
    match serde_json::from_value::<NetworkConfigureParams>(params) {
        Ok(p) => {
            info!(
                "Configuring network: {} with IP {}/{}",
                p.interface, p.ip, p.netmask
            );

            // Configure network interface
            use std::process::Command;

            // Add IP address
            let status = Command::new("ip")
                .args([
                    "addr",
                    "add",
                    &format!("{}/{}", p.ip, p.netmask),
                    "dev",
                    &p.interface,
                ])
                .status();

            if let Err(e) = status {
                return Message::error(
                    id,
                    error_codes::NETWORK_CONFIG_FAILED,
                    format!("Failed to configure IP: {}", e),
                );
            }

            // Bring interface up
            let status = Command::new("ip")
                .args(["link", "set", &p.interface, "up"])
                .status();

            if let Err(e) = status {
                return Message::error(
                    id,
                    error_codes::NETWORK_CONFIG_FAILED,
                    format!("Failed to bring interface up: {}", e),
                );
            }

            // Configure gateway if provided
            if let Some(gateway) = p.gateway {
                let status = Command::new("ip")
                    .args(["route", "add", "default", "via", &gateway.to_string()])
                    .status();

                if let Err(e) = status {
                    warn!("Failed to configure gateway: {}", e);
                }
            }

            Message::success(id, serde_json::json!({"configured": true}))
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid params: {}", e),
        ),
    }
}

/// Handle network_status request
async fn handle_network_status(id: String, params: serde_json::Value) -> Message {
    let interface = params
        .get("interface")
        .and_then(|v| v.as_str())
        .unwrap_or("usb0");

    info!("Getting network status for {}", interface);

    // Get interface status using ip command
    use std::process::Command;

    let output = Command::new("ip")
        .args(["addr", "show", interface])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Parse basic info (simplified parser)
            let up = stdout.contains("state UP");
            let ip = parse_ip_from_output(&stdout);
            let mac = parse_mac_from_output(&stdout);

            let status = NetworkStatusData {
                interface: interface.to_string(),
                up,
                ip,
                netmask: Some(24), // Simplified
                mac,
                speed: None,
            };

            Message::success(id, serde_json::to_value(status).unwrap())
        }
        Err(e) => Message::error(
            id,
            error_codes::INTERNAL_ERROR,
            format!("Failed to get network status: {}", e),
        ),
    }
}

/// Parse IP address from ip command output
fn parse_ip_from_output(output: &str) -> Option<std::net::IpAddr> {
    for line in output.lines() {
        if line.contains("inet ") || line.contains("inet6 ") {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                if let Some(ip_part) = parts[1].split('/').next() {
                    if let Ok(ip) = ip_part.parse() {
                        return Some(ip);
                    }
                }
            }
        }
    }
    None
}

/// Parse MAC address from ip command output
fn parse_mac_from_output(output: &str) -> Option<String> {
    for line in output.lines() {
        if line.contains("link/ether") {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                return Some(parts[1].to_string());
            }
        }
    }
    None
}

/// USB Hub handlers

/// Convert UsbHub to HubInfo
fn usb_hub_to_hub_info(hub: testctl::UsbHub) -> HubInfo {
    HubInfo {
        id: hub.id,
        vendor_id: hub.vendor_id,
        product_id: hub.product_id,
        location: hub.location,
        name: hub.name,
        ports: hub.ports.into_iter().map(usb_port_to_hub_port_info).collect(),
    }
}

/// Convert UsbPort to HubPortInfo
fn usb_port_to_hub_port_info(port: testctl::UsbPort) -> HubPortInfo {
    HubPortInfo {
        port_number: port.port_number,
        power_state: match port.power_state {
            testctl::PowerState::On => HubPortPowerState::On,
            testctl::PowerState::Off => HubPortPowerState::Off,
        },
        connected_device: port.connected_device,
        reservation: port.reservation.map(|r| HubPortReservationInfo {
            reserved_by: match r.reserved_by {
                testctl::usb_hub::types::DeviceRole::Master => HubDeviceRole::Master,
                testctl::usb_hub::types::DeviceRole::Uut => HubDeviceRole::Uut,
                testctl::usb_hub::types::DeviceRole::Slave => HubDeviceRole::Slave,
            },
            reserved_at: r.reserved_at.duration_since(std::time::UNIX_EPOCH)
                .map(|d| chrono::Utc::now() - chrono::Duration::seconds(d.as_secs() as i64))
                .unwrap_or_else(|_| chrono::Utc::now()),
            expires_at: r.expires_at.and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| chrono::Utc::now() - chrono::Duration::seconds(d.as_secs() as i64))
            }),
            device_id: r.device_id,
        }),
    }
}

/// Convert HubDeviceRole to testctl hub DeviceRole
fn hub_device_role_to_usb_hub_role(role: HubDeviceRole) -> testctl::usb_hub::types::DeviceRole {
    match role {
        HubDeviceRole::Master => testctl::usb_hub::types::DeviceRole::Master,
        HubDeviceRole::Uut => testctl::usb_hub::types::DeviceRole::Uut,
        HubDeviceRole::Slave => testctl::usb_hub::types::DeviceRole::Slave,
    }
}

/// Handle hub list request
async fn handle_hub_list(id: String, state: &Arc<RwLock<RemoteState>>) -> Message {
    let state = state.read().await;
    let hubs = state.hub_manager.list_hubs().await;
    let hub_infos: Vec<HubInfo> = hubs.into_iter().map(usb_hub_to_hub_info).collect();

    Message::success(
        id,
        serde_json::to_value(HubListResult { hubs: hub_infos }).unwrap(),
    )
}

/// Handle hub discover request
async fn handle_hub_discover(id: String, state: &Arc<RwLock<RemoteState>>) -> Message {
    let state = state.read().await;

    if !state.hub_manager.is_uhubctl_available() {
        return Message::error(
            id,
            error_codes::UHUBCTL_NOT_AVAILABLE,
            "uhubctl is not available on this system".to_string(),
        );
    }

    match state.hub_manager.discover_hubs().await {
        Ok(hubs) => {
            let hub_infos: Vec<HubInfo> = hubs.iter().map(|h| usb_hub_to_hub_info(h.clone())).collect();
            Message::success(
                id,
                serde_json::to_value(HubDiscoverResult {
                    discovered: hub_infos.len(),
                    hubs: hub_infos,
                })
                .unwrap(),
            )
        }
        Err(e) => Message::error(
            id,
            error_codes::INTERNAL_ERROR,
            format!("Failed to discover hubs: {}", e),
        ),
    }
}

/// Handle hub status request
async fn handle_hub_status(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<HubStatusParams>(params) {
        Ok(p) => {
            let state = state.read().await;
            match state.hub_manager.refresh_hub(&p.hub_id).await {
                Ok(hub) => {
                    let hub_info = usb_hub_to_hub_info(hub);
                    Message::success(id, serde_json::to_value(hub_info).unwrap())
                }
                Err(e) => {
                    let error_code = match e {
                        testctl::usb_hub::manager::ManagerError::HubNotFound(_) => {
                            error_codes::HUB_NOT_FOUND
                        }
                        _ => error_codes::INTERNAL_ERROR,
                    };
                    Message::error(id, error_code, format!("Failed to get hub status: {}", e))
                }
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid parameters: {}", e),
        ),
    }
}

/// Handle hub port power on request
async fn handle_hub_port_power_on(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<HubPortPowerOnParams>(params) {
        Ok(p) => {
            let state = state.read().await;
            match state.hub_manager.power_on_port(&p.hub_id, p.port).await {
                Ok(()) => Message::success(id, serde_json::json!({"status": "ok"})),
                Err(e) => {
                    let error_code = match e {
                        testctl::usb_hub::manager::ManagerError::HubNotFound(_) => {
                            error_codes::HUB_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::PortNotFound(_) => {
                            error_codes::HUB_PORT_NOT_FOUND
                        }
                        _ => error_codes::HUB_POWER_CONTROL_FAILED,
                    };
                    Message::error(id, error_code, format!("Failed to power on port: {}", e))
                }
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid parameters: {}", e),
        ),
    }
}

/// Handle hub port power off request
async fn handle_hub_port_power_off(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<HubPortPowerOffParams>(params) {
        Ok(p) => {
            let state = state.read().await;
            match state.hub_manager.power_off_port(&p.hub_id, p.port).await {
                Ok(()) => Message::success(id, serde_json::json!({"status": "ok"})),
                Err(e) => {
                    let error_code = match e {
                        testctl::usb_hub::manager::ManagerError::HubNotFound(_) => {
                            error_codes::HUB_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::PortNotFound(_) => {
                            error_codes::HUB_PORT_NOT_FOUND
                        }
                        _ => error_codes::HUB_POWER_CONTROL_FAILED,
                    };
                    Message::error(id, error_code, format!("Failed to power off port: {}", e))
                }
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid parameters: {}", e),
        ),
    }
}

/// Handle hub port reserve request
async fn handle_hub_port_reserve(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<HubPortReserveParams>(params) {
        Ok(p) => {
            let state = state.read().await;
            let duration = p.duration_secs.map(std::time::Duration::from_secs);
            let requester = hub_device_role_to_usb_hub_role(p.requester);

            let result = if p.power_on {
                state
                    .hub_manager
                    .reserve_and_power_on(&p.hub_id, p.port, requester, duration)
                    .await
            } else {
                state
                    .hub_manager
                    .reserve_port(&p.hub_id, p.port, requester, duration)
                    .await
            };

            match result {
                Ok(()) => Message::success(id, serde_json::json!({"status": "reserved"})),
                Err(e) => {
                    let error_code = match e {
                        testctl::usb_hub::manager::ManagerError::HubNotFound(_) => {
                            error_codes::HUB_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::PortNotFound(_) => {
                            error_codes::HUB_PORT_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::PortAlreadyReserved(_) => {
                            error_codes::HUB_PORT_ALREADY_RESERVED
                        }
                        _ => error_codes::HUB_INVALID_OPERATION,
                    };
                    Message::error(id, error_code, format!("Failed to reserve port: {}", e))
                }
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid parameters: {}", e),
        ),
    }
}

/// Handle hub port release request
async fn handle_hub_port_release(
    id: String,
    params: serde_json::Value,
    state: &Arc<RwLock<RemoteState>>,
) -> Message {
    match serde_json::from_value::<HubPortReleaseParams>(params) {
        Ok(p) => {
            let state = state.read().await;
            let releaser = hub_device_role_to_usb_hub_role(p.releaser);

            let result = if p.power_off {
                state
                    .hub_manager
                    .power_off_and_release(&p.hub_id, p.port, releaser)
                    .await
            } else {
                state
                    .hub_manager
                    .release_port(&p.hub_id, p.port, releaser)
                    .await
            };

            match result {
                Ok(()) => Message::success(id, serde_json::json!({"status": "released"})),
                Err(e) => {
                    let error_code = match e {
                        testctl::usb_hub::manager::ManagerError::HubNotFound(_) => {
                            error_codes::HUB_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::PortNotFound(_) => {
                            error_codes::HUB_PORT_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::ReservationNotFound => {
                            error_codes::HUB_RESERVATION_NOT_FOUND
                        }
                        testctl::usb_hub::manager::ManagerError::InvalidOperation(_) => {
                            error_codes::HUB_INVALID_OPERATION
                        }
                        _ => error_codes::INTERNAL_ERROR,
                    };
                    Message::error(id, error_code, format!("Failed to release port: {}", e))
                }
            }
        }
        Err(e) => Message::error(
            id,
            error_codes::INVALID_PARAMS,
            format!("Invalid parameters: {}", e),
        ),
    }
}

/// Get system uptime in seconds
fn get_uptime() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/uptime") {
            if let Some(uptime_str) = contents.split_whitespace().next() {
                if let Ok(uptime) = uptime_str.parse::<f64>() {
                    return uptime as u64;
                }
            }
        }
    }
    0
}
