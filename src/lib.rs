// Test Control Protocol Library
//
// Protocol for coordinating tests between master and remote devices.
// Supports USB gadget networking for communication with UUT and slave devices.

pub mod protocol;
pub mod usb_hub;

// Re-export commonly used types
pub use protocol::{
    DeviceInfo, DeviceRole, ErrorInfo, Event, EventType, LogEntry, LogLevel, Message, Method,
    NetworkConfigureParams, NetworkStatusData, Request, Response, ResponseResult,
    SetRoleParams, TestAbortParams, TestResultsData, TestResultsParams, TestStartParams,
    TestStartResult, TestStatus, error_codes,
    // USB Hub types
    HubDeviceRole, HubInfo, HubPortInfo, HubPortPowerState, HubPortReservationInfo,
    HubListResult, HubDiscoverResult, HubStatusParams, HubPortPowerOnParams,
    HubPortPowerOffParams, HubPortReserveParams, HubPortReleaseParams,
};

pub use protocol::framing::{read_message, write_message};
pub use protocol::transport::{
    Transport, TransportServer, UsbGadgetConfig, setup_usb_gadget, setup_usb_gadget_device,
    DEFAULT_TCP_PORT,
};

pub use usb_hub::{UsbHubManager, UsbHub, UsbPort, PowerState, PortReservation};
