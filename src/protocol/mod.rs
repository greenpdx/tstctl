// Test Control Protocol
//
// Similar to JSON-RPC 2.0 but specialized for test automation.
// Inspired by Chef automation protocol.

use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

pub mod framing;
pub mod transport;

/// Protocol version
pub const PROTOCOL_VERSION: &str = "1.0";

/// Maximum message size (16 MB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Test Control Protocol Message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    /// Request from master to remote
    Request(Request),
    /// Response from remote to master
    Response(Response),
    /// Event notification (async, no response expected)
    Event(Event),
}

/// Request from master to remote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Unique request ID
    pub id: String,
    /// Request method
    pub method: Method,
    /// Request parameters
    pub params: serde_json::Value,
}

/// Request methods
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Start a test
    TestStart,
    /// Abort a running test
    TestAbort,
    /// Cancel a test
    TestCancel,
    /// Get test results
    TestResults,
    /// Get test status
    TestStatus,
    /// Configure network interface
    NetworkConfigure,
    /// Get network status
    NetworkStatus,
    /// Set device role (uut or slave)
    SetRole,
    /// Ping (keepalive)
    Ping,
    /// Get device info
    GetInfo,

    // USB Hub control methods
    /// List available USB hubs
    HubList,
    /// Get status of a specific hub
    HubStatus,
    /// Power on a hub port
    HubPortPowerOn,
    /// Power off a hub port
    HubPortPowerOff,
    /// Reserve a hub port for exclusive access
    HubPortReserve,
    /// Release a hub port reservation
    HubPortRelease,
    /// Discover and refresh hub list
    HubDiscover,
}

/// Response from remote to master
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// Request ID this responds to
    pub id: String,
    /// Response result
    #[serde(flatten)]
    pub result: ResponseResult,
}

/// Response result (success or error)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseResult {
    Success { result: serde_json::Value },
    Error { error: ErrorInfo },
}

/// Error information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Event notification (async)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event type
    pub event: EventType,
    /// Event data
    pub data: serde_json::Value,
    /// Event timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Test started
    TestStarted,
    /// Test progress update
    TestProgress,
    /// Test completed
    TestCompleted,
    /// Test failed
    TestFailed,
    /// Log message
    Log,
    /// Network interface status changed
    NetworkChanged,
    /// Device error
    DeviceError,

    // USB Hub events
    /// USB hub connected
    HubConnected,
    /// USB hub disconnected
    HubDisconnected,
    /// USB hub port power state changed
    HubPortPowerChanged,
    /// USB device connected to hub port
    HubDeviceConnected,
    /// USB device disconnected from hub port
    HubDeviceDisconnected,
    /// USB hub port reservation expired
    HubReservationExpired,
}

// Request parameter types

/// TestStart parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStartParams {
    /// Test suite name
    pub suite: String,
    /// Test case name (optional, runs all if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_case: Option<String>,
    /// Test configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    /// Timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// TestAbort parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestAbortParams {
    /// Test ID to abort
    pub test_id: String,
    /// Reason for abort
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// TestResults parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultsParams {
    /// Test ID
    pub test_id: String,
    /// Include detailed logs
    #[serde(default)]
    pub include_logs: bool,
}

/// NetworkConfigure parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfigureParams {
    /// Interface name (e.g., "usb0")
    pub interface: String,
    /// IP address
    pub ip: IpAddr,
    /// Netmask (CIDR notation, e.g., 24)
    pub netmask: u8,
    /// Gateway (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<IpAddr>,
}

/// SetRole parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetRoleParams {
    /// Device role
    pub role: DeviceRole,
}

/// Device role
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceRole {
    /// Unit Under Test
    Uut,
    /// Slave device (helper)
    Slave,
}

// USB Hub request parameters

/// HubStatus parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubStatusParams {
    /// Hub ID
    pub hub_id: String,
}

/// HubPortPowerOn parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortPowerOnParams {
    /// Hub ID
    pub hub_id: String,
    /// Port number
    pub port: u8,
}

/// HubPortPowerOff parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortPowerOffParams {
    /// Hub ID
    pub hub_id: String,
    /// Port number
    pub port: u8,
}

/// HubPortReserve parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortReserveParams {
    /// Hub ID
    pub hub_id: String,
    /// Port number
    pub port: u8,
    /// Requesting device role
    pub requester: HubDeviceRole,
    /// Optional reservation duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    /// Also power on the port
    #[serde(default)]
    pub power_on: bool,
}

/// HubPortRelease parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortReleaseParams {
    /// Hub ID
    pub hub_id: String,
    /// Port number
    pub port: u8,
    /// Releasing device role
    pub releaser: HubDeviceRole,
    /// Also power off the port
    #[serde(default)]
    pub power_off: bool,
}

/// Hub device role (includes Master)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum HubDeviceRole {
    /// Master test coordinator
    Master,
    /// Unit Under Test
    Uut,
    /// Slave device (helper)
    Slave,
}

// Response result types

/// TestStart result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStartResult {
    /// Assigned test ID
    pub test_id: String,
    /// Test status
    pub status: TestStatus,
}

/// Test status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    /// Test is queued
    Queued,
    /// Test is running
    Running,
    /// Test completed successfully
    Passed,
    /// Test failed
    Failed,
    /// Test was aborted
    Aborted,
    /// Test was cancelled
    Cancelled,
}

/// TestResults result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResultsData {
    /// Test ID
    pub test_id: String,
    /// Test status
    pub status: TestStatus,
    /// Test suite name
    pub suite: String,
    /// Test case name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_case: Option<String>,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End time (if completed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Duration in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Number of assertions passed
    pub passed: u32,
    /// Number of assertions failed
    pub failed: u32,
    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Test logs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logs: Option<Vec<LogEntry>>,
}

/// Log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub level: LogLevel,
    pub message: String,
}

/// Log level
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// NetworkStatus result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStatusData {
    /// Interface name
    pub interface: String,
    /// Interface is up
    pub up: bool,
    /// IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<IpAddr>,
    /// Netmask
    #[serde(skip_serializing_if = "Option::is_none")]
    pub netmask: Option<u8>,
    /// MAC address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    /// Link speed in Mbps
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u32>,
}

/// GetInfo result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Device hostname
    pub hostname: String,
    /// Device role
    pub role: DeviceRole,
    /// Protocol version
    pub protocol_version: String,
    /// Software version
    pub version: String,
    /// Uptime in seconds
    pub uptime: u64,
    /// Available test suites
    pub test_suites: Vec<String>,
}

// USB Hub response result types

/// USB Hub information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubInfo {
    pub id: String,
    pub vendor_id: String,
    pub product_id: String,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub ports: Vec<HubPortInfo>,
}

/// USB Hub port information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortInfo {
    pub port_number: u8,
    pub power_state: HubPortPowerState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connected_device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reservation: Option<HubPortReservationInfo>,
}

/// Hub port power state
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HubPortPowerState {
    On,
    Off,
}

/// Hub port reservation information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubPortReservationInfo {
    pub reserved_by: HubDeviceRole,
    pub reserved_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// HubList result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubListResult {
    pub hubs: Vec<HubInfo>,
}

/// HubDiscover result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubDiscoverResult {
    pub discovered: usize,
    pub hubs: Vec<HubInfo>,
}

// Error codes
pub mod error_codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Custom error codes
    pub const TEST_NOT_FOUND: i32 = -32000;
    pub const TEST_ALREADY_RUNNING: i32 = -32001;
    pub const NETWORK_CONFIG_FAILED: i32 = -32002;
    pub const PERMISSION_DENIED: i32 = -32003;
    pub const TIMEOUT: i32 = -32004;
    pub const USB_GADGET_ERROR: i32 = -32005;

    // USB Hub error codes
    pub const HUB_NOT_FOUND: i32 = -32100;
    pub const HUB_PORT_NOT_FOUND: i32 = -32101;
    pub const HUB_PORT_ALREADY_RESERVED: i32 = -32102;
    pub const HUB_RESERVATION_NOT_FOUND: i32 = -32103;
    pub const HUB_POWER_CONTROL_FAILED: i32 = -32104;
    pub const UHUBCTL_NOT_AVAILABLE: i32 = -32105;
    pub const HUB_INVALID_OPERATION: i32 = -32106;
}

impl Message {
    /// Create a request message
    pub fn request(method: Method, params: serde_json::Value) -> Self {
        Message::Request(Request {
            id: Uuid::new_v4().to_string(),
            method,
            params,
        })
    }

    /// Create a success response
    pub fn success(id: String, result: serde_json::Value) -> Self {
        Message::Response(Response {
            id,
            result: ResponseResult::Success { result },
        })
    }

    /// Create an error response
    pub fn error(id: String, code: i32, message: String) -> Self {
        Message::Response(Response {
            id,
            result: ResponseResult::Error {
                error: ErrorInfo {
                    code,
                    message,
                    data: None,
                },
            },
        })
    }

    /// Create an event message
    pub fn event(event: EventType, data: serde_json::Value) -> Self {
        Message::Event(Event {
            event,
            data,
            timestamp: chrono::Utc::now(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = Message::request(
            Method::TestStart,
            serde_json::json!({
                "suite": "api_tests",
                "test_case": "test_dns_status"
            }),
        );

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();

        match decoded {
            Message::Request(req) => {
                assert!(matches!(req.method, Method::TestStart));
            }
            _ => panic!("Expected Request"),
        }
    }

    #[test]
    fn test_response_success() {
        let msg = Message::success(
            "test-123".to_string(),
            serde_json::json!({"status": "ok"}),
        );

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();

        match decoded {
            Message::Response(resp) => match resp.result {
                ResponseResult::Success { .. } => {}
                _ => panic!("Expected success"),
            },
            _ => panic!("Expected Response"),
        }
    }

    #[test]
    fn test_event() {
        let msg = Message::event(
            EventType::TestProgress,
            serde_json::json!({"progress": 50}),
        );

        let json = serde_json::to_string(&msg).unwrap();
        let decoded: Message = serde_json::from_str(&json).unwrap();

        match decoded {
            Message::Event(evt) => {
                assert!(matches!(evt.event, EventType::TestProgress));
            }
            _ => panic!("Expected Event"),
        }
    }
}
