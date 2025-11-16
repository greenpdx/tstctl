# TestCtl - Test Control Protocol

Rust implementation of a test control protocol for coordinating automated tests between a master controller and remote devices (UUT and slaves) over USB gadget networking.

## Architecture

```
┌──────────┐
│  Master  │ (Test coordinator - Linux PC or Raspberry Pi)
└────┬─────┘
     │
     ├─── USB Gadget (usb0) ──→ ┌──────┐
     │                           │ UUT  │ (Unit Under Test - OpenWrt device)
     │                           └──────┘
     │
     └─── USB Gadget (usb1) ──→ ┌──────┐
                                 │Slave │ (Helper device - OpenWrt device)
                                 └──────┘
```

## Features

- **USB Gadget Networking**: Direct USB connections between master and devices
- **Test Coordination**: Start, abort, cancel tests remotely
- **Network Configuration**: Configure interfaces on remote devices
- **Real-time Events**: Async event notifications during test execution
- **Protocol**: JSON-based, length-prefixed messages (similar to JSON-RPC 2.0)

## Protocol Commands

### Master → Remote

- `test.start` - Start a test suite/case
- `test.abort` - Abort running test
- `test.cancel` - Cancel test
- `test.results` - Get test results
- `test.status` - Get test status
- `network.configure` - Configure network interface
- `network.status` - Get network status
- `set_role` - Set device role (uut/slave)
- `ping` - Keepalive
- `get_info` - Get device information

### Remote → Master

- Events: `test_started`, `test_progress`, `test_completed`, `test_failed`, `log`, `network_changed`

## Building

```bash
# Build all components
cargo build --release -p testctl

# Build specific binary
cargo build --release --bin testctl-master
cargo build --release --bin testctl-remote
```

## Usage

### On Remote Device (UUT or Slave)

```bash
# Setup USB gadget device (one-time, requires root)
sudo testctl-remote --setup-gadget

# Run remote as UUT
sudo testctl-remote --role uut

# Run remote as Slave
sudo testctl-remote --role slave

# TCP mode (for testing without USB)
testctl-remote --tcp --tcp-addr 0.0.0.0:9999
```

### On Master

```bash
# Ping remote device
testctl-master ping

# Get device info
testctl-master info

# Start a test
testctl-master test-start api_tests --test-case test_dns_status

# Get test results
testctl-master test-results test-1 --logs

# Configure network on remote
testctl-master network-configure usb0 192.168.100.2 24

# Interactive session
testctl-master interactive

# TCP mode (for testing)
testctl-master --tcp --remote-addr 192.168.100.2:9999 ping
```

## USB Gadget Setup

The protocol uses USB gadget networking which allows devices to communicate over USB as if they were on an Ethernet network.

### On Linux (Master)

USB gadget client mode requires a device that supports USB OTG/gadget. For testing, you can use:
- Raspberry Pi (Zero, 4, 5)
- Any Linux device with USB gadget support

### On OpenWrt (Remote - UUT/Slave)

Most OpenWrt devices support USB gadget mode. The `--setup-gadget` flag configures:

1. Loads kernel modules (if needed)
2. Configures USB gadget via configfs
3. Sets up RNDIS (USB networking) function
4. Assigns IP addresses
5. Brings up interface

**Default Network Configuration:**
- Master: 192.168.100.1/24
- Remote: 192.168.100.2/24
- Interface: usb0

## Protocol Specification

### Message Format

All messages use length-prefixed framing:

```
[4-byte length (big-endian)][JSON payload]
```

### Message Types

**Request:**
```json
{
  "type": "Request",
  "id": "uuid-1234",
  "method": "test_start",
  "params": {
    "suite": "api_tests",
    "test_case": "test_dns_status"
  }
}
```

**Response (Success):**
```json
{
  "type": "Response",
  "id": "uuid-1234",
  "result": {
    "test_id": "test-1",
    "status": "running"
  }
}
```

**Response (Error):**
```json
{
  "type": "Response",
  "id": "uuid-1234",
  "error": {
    "code": -32000,
    "message": "Test not found"
  }
}
```

**Event:**
```json
{
  "type": "Event",
  "event": "test_progress",
  "data": {
    "test_id": "test-1",
    "progress": 50
  },
  "timestamp": "2025-11-16T12:34:56.789Z"
}
```

## Error Codes

- `-32700` - Parse error
- `-32600` - Invalid request
- `-32601` - Method not found
- `-32602` - Invalid params
- `-32603` - Internal error
- `-32000` - Test not found
- `-32001` - Test already running
- `-32002` - Network config failed
- `-32003` - Permission denied
- `-32004` - Timeout
- `-32005` - USB gadget error

## Examples

### Start Test and Monitor Progress

```bash
# Start test
testctl-master test-start api_tests --test-case test_dns_status

# Output:
# {
#   "test_id": "test-1",
#   "status": "running"
# }

# Get results
testctl-master test-results test-1 --logs

# Output:
# {
#   "test_id": "test-1",
#   "status": "passed",
#   "suite": "api_tests",
#   "test_case": "test_dns_status",
#   "passed": 5,
#   "failed": 0,
#   "duration": 2.34
# }
```

### Configure Network

```bash
# Configure usb0 on remote device
testctl-master network-configure usb0 192.168.100.2 24 --gateway 192.168.100.1

# Check status
testctl-master network-status usb0
```

## Development

### Testing with TCP (No USB Hardware)

For development without USB hardware, use TCP mode:

**Terminal 1 (Remote):**
```bash
testctl-remote --tcp --tcp-addr 127.0.0.1:9999
```

**Terminal 2 (Master):**
```bash
testctl-master --tcp --remote-addr 127.0.0.1:9999 ping
```

### Running Tests

```bash
cargo test -p testctl
```

## Future Enhancements

Inspired by Chef automation protocol, future versions may include:

- Multi-device orchestration (parallel test execution)
- Test result aggregation and reporting
- Device provisioning and configuration management
- Firmware updates via USB
- Screenshot/video capture from UUT
- Power control integration
- Serial console access
- Recovery and failsafe modes

## License

Same as parent project (crrouter_web)
