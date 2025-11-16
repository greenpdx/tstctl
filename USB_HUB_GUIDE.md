# USB Hub Multi-Device Access Guide

This guide explains how to use USB hubs with the testctl system to share USB devices between the master, UUT (Unit Under Test), and slave devices.

## Overview

The USB hub functionality allows multiple devices (master, UUT, and slaves) to access USB devices connected to a shared USB hub. The system provides:

- **Port power control** - Turn USB ports on/off individually
- **Port reservation** - Reserve ports for exclusive access
- **Resource coordination** - Prevent conflicts between devices
- **Automatic discovery** - Find and enumerate USB hubs

## Architecture

```
┌──────────┐
│  Master  │ ← Can reserve and control hub ports
└────┬─────┘
     │
     ├─── USB Gadget ──→ ┌──────┐
     │                    │ UUT  │ ← Can request hub port access
     │                    └──────┘
     │
     └─── USB Gadget ──→ ┌──────┐
                          │Slave │ ← Can request hub port access
                          └──────┘

All devices can access:
┌─────────────┐
│  USB Hub    │
├─────────────┤
│ Port 1: Off │ ← Available
│ Port 2: On  │ ← Reserved by Master
│ Port 3: On  │ ← Reserved by UUT
│ Port 4: Off │ ← Reserved by Slave
└─────────────┘
```

## Prerequisites

### Install uhubctl

The system requires `uhubctl` for USB hub port power control.

**On Debian/Ubuntu:**
```bash
sudo apt-get install uhubctl
```

**From source:**
```bash
git clone https://github.com/mvp/uhubctl
cd uhubctl
make
sudo make install
```

**Verify installation:**
```bash
uhubctl --version
```

### Supported USB Hubs

Not all USB hubs support per-port power switching. Check uhubctl's compatibility list:
https://github.com/mvp/uhubctl#compatible-usb-hubs

Popular compatible hubs include:
- D-Link DUB-H7 (7-port USB 2.0)
- Cypress CY4608 (4-port USB 2.0)
- Most Raspberry Pi built-in hubs

## Quick Start

### 1. Discover USB Hubs

From the master device:

```bash
# Using CLI
testctl-master hub discover

# Or in interactive mode
testctl-master interactive
> hub discover
```

Example output:
```json
{
  "discovered": 1,
  "hubs": [
    {
      "id": "2109:2817",
      "vendor_id": "2109",
      "product_id": "2817",
      "location": "1-1.4",
      "name": null,
      "ports": [
        {"port_number": 1, "power_state": "on"},
        {"port_number": 2, "power_state": "on"},
        {"port_number": 3, "power_state": "on"},
        {"port_number": 4, "power_state": "on"}
      ]
    }
  ]
}
```

### 2. Reserve a Port

Reserve port 1 for the master:

```bash
testctl-master hub reserve 2109:2817 1 --role master --power-on
```

Reserve port 2 for the UUT:

```bash
testctl-master hub reserve 2109:2817 2 --role uut --duration 300
```

The `--duration` flag specifies reservation time in seconds (optional).

### 3. Control Port Power

Power on a port:
```bash
testctl-master hub on 2109:2817 3
```

Power off a port:
```bash
testctl-master hub off 2109:2817 3
```

### 4. Release a Port

Release a reserved port:
```bash
testctl-master hub release 2109:2817 1 --role master --power-off
```

## Command Reference

### Master CLI Commands

```bash
# List all discovered hubs
testctl-master hub list

# Discover and refresh hub list
testctl-master hub discover

# Get hub status
testctl-master hub status <hub_id>

# Power control
testctl-master hub on <hub_id> <port>
testctl-master hub off <hub_id> <port>

# Reservation management
testctl-master hub reserve <hub_id> <port> [OPTIONS]
  --role <master|uut|slave>    # Device role (default: master)
  --duration <seconds>          # Reservation duration (optional)
  --power-on                    # Also power on the port

testctl-master hub release <hub_id> <port> [OPTIONS]
  --role <master|uut|slave>    # Device role (default: master)
  --power-off                   # Also power off the port
```

### Interactive Mode

Start interactive session:
```bash
testctl-master interactive
```

Hub commands in interactive mode:
```
> hub list                      # List all hubs
> hub discover                  # Discover hubs
> hub status <hub_id>           # Get hub status
> hub on <hub_id> <port>        # Power on port
> hub off <hub_id> <port>       # Power off port
> hub reserve <hub_id> <port>   # Reserve port
> hub release <hub_id> <port>   # Release port
```

## Protocol API

The USB hub functionality extends the testctl protocol with new methods.

### Methods

#### `hub_list`
List all registered USB hubs.

**Request:**
```json
{
  "type": "Request",
  "id": "req-123",
  "method": "hub_list",
  "params": {}
}
```

**Response:**
```json
{
  "type": "Response",
  "id": "req-123",
  "result": {
    "hubs": [
      {
        "id": "2109:2817",
        "vendor_id": "2109",
        "product_id": "2817",
        "location": "1-1.4",
        "name": null,
        "ports": [...]
      }
    ]
  }
}
```

#### `hub_discover`
Discover and refresh the list of USB hubs.

**Request:**
```json
{
  "type": "Request",
  "id": "req-124",
  "method": "hub_discover",
  "params": {}
}
```

**Response:**
```json
{
  "type": "Response",
  "id": "req-124",
  "result": {
    "discovered": 2,
    "hubs": [...]
  }
}
```

#### `hub_status`
Get the current status of a specific hub.

**Request:**
```json
{
  "type": "Request",
  "id": "req-125",
  "method": "hub_status",
  "params": {
    "hub_id": "2109:2817"
  }
}
```

#### `hub_port_power_on`
Power on a specific port.

**Request:**
```json
{
  "type": "Request",
  "id": "req-126",
  "method": "hub_port_power_on",
  "params": {
    "hub_id": "2109:2817",
    "port": 1
  }
}
```

#### `hub_port_power_off`
Power off a specific port.

**Request:**
```json
{
  "type": "Request",
  "id": "req-127",
  "method": "hub_port_power_off",
  "params": {
    "hub_id": "2109:2817",
    "port": 1
  }
}
```

#### `hub_port_reserve`
Reserve a port for exclusive access.

**Request:**
```json
{
  "type": "Request",
  "id": "req-128",
  "method": "hub_port_reserve",
  "params": {
    "hub_id": "2109:2817",
    "port": 1,
    "requester": "master",
    "duration_secs": 300,
    "power_on": true
  }
}
```

**Response:**
```json
{
  "type": "Response",
  "id": "req-128",
  "result": {
    "status": "reserved"
  }
}
```

#### `hub_port_release`
Release a port reservation.

**Request:**
```json
{
  "type": "Request",
  "id": "req-129",
  "method": "hub_port_release",
  "params": {
    "hub_id": "2109:2817",
    "port": 1,
    "releaser": "master",
    "power_off": false
  }
}
```

**Response:**
```json
{
  "type": "Response",
  "id": "req-129",
  "result": {
    "status": "released"
  }
}
```

### Event Types

The system emits events for hub state changes:

```rust
pub enum EventType {
    // USB Hub events
    HubConnected,           // USB hub connected
    HubDisconnected,        // USB hub disconnected
    HubPortPowerChanged,    // Port power state changed
    HubDeviceConnected,     // Device connected to port
    HubDeviceDisconnected,  // Device disconnected from port
    HubReservationExpired,  // Port reservation expired
}
```

### Error Codes

USB hub-specific error codes:

```rust
HUB_NOT_FOUND: -32100              // Hub ID not found
HUB_PORT_NOT_FOUND: -32101         // Port number not found
HUB_PORT_ALREADY_RESERVED: -32102  // Port already reserved
HUB_RESERVATION_NOT_FOUND: -32103  // No reservation found
HUB_POWER_CONTROL_FAILED: -32104   // Power control failed
UHUBCTL_NOT_AVAILABLE: -32105      // uhubctl not installed
HUB_INVALID_OPERATION: -32106      // Invalid operation
```

## Usage Examples

### Example 1: Shared USB Storage

Test both UUT and slave accessing the same USB storage device:

```bash
# 1. Master discovers the hub
testctl-master hub discover

# 2. UUT reserves port 1 with USB storage
testctl-master hub reserve 2109:2817 1 --role uut --duration 60 --power-on

# 3. UUT runs storage tests (60 seconds)
testctl-master start storage_tests

# 4. UUT releases the port
testctl-master hub release 2109:2817 1 --role uut

# 5. Slave reserves the same port
testctl-master hub reserve 2109:2817 1 --role slave --duration 60 --power-on

# 6. Slave runs storage tests
# ... (send command to slave)

# 7. Slave releases the port
testctl-master hub release 2109:2817 1 --role slave --power-off
```

### Example 2: Power Cycling Device

```bash
# Power off the device
testctl-master hub off 2109:2817 2

# Wait 5 seconds
sleep 5

# Power on the device
testctl-master hub on 2109:2817 2
```

### Example 3: Multi-Device Test Orchestration

```python
import json
import subprocess

def run_hub_command(cmd):
    result = subprocess.run(
        ["testctl-master", "hub"] + cmd,
        capture_output=True,
        text=True
    )
    return json.loads(result.stdout)

# Discover hubs
hubs = run_hub_command(["discover"])
hub_id = hubs["hubs"][0]["id"]

# Test scenario: All devices access different ports simultaneously
ports = {
    "master": 1,
    "uut": 2,
    "slave": 3
}

# Reserve all ports
for role, port in ports.items():
    run_hub_command([
        "reserve", hub_id, str(port),
        "--role", role,
        "--power-on"
    ])

# Run tests in parallel
# ... your test code here ...

# Release all ports
for role, port in ports.items():
    run_hub_command([
        "release", hub_id, str(port),
        "--role", role,
        "--power-off"
    ])
```

## Programmatic Access (Rust)

### Using the UsbHubManager

```rust
use testctl::{UsbHubManager, usb_hub::types::DeviceRole};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create hub manager
    let hub_manager = UsbHubManager::new(None);

    // Discover hubs
    let hubs = hub_manager.discover_hubs().await?;
    println!("Found {} hubs", hubs.len());

    if let Some(hub) = hubs.first() {
        let hub_id = &hub.id;

        // Reserve port 1 for master
        hub_manager.reserve_and_power_on(
            hub_id,
            1,
            DeviceRole::Master,
            Some(Duration::from_secs(60))
        ).await?;

        // Do work with the port
        println!("Port 1 reserved and powered on");

        // Release the port
        hub_manager.power_off_and_release(
            hub_id,
            1,
            DeviceRole::Master
        ).await?;

        println!("Port 1 released and powered off");
    }

    Ok(())
}
```

## Troubleshooting

### uhubctl not found

**Error:**
```
Error -32105: uhubctl is not available on this system
```

**Solution:**
Install uhubctl (see Prerequisites section above).

### Permission denied

**Error:**
```
Failed to execute uhubctl: Permission denied
```

**Solution:**
Run with sudo or add udev rules:

```bash
# Create udev rule
sudo tee /etc/udev/rules.d/52-usb-hub.rules <<EOF
SUBSYSTEM=="usb", ATTR{idVendor}=="2109", ATTR{idProduct}=="2817", MODE="0666"
EOF

# Reload udev rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

Replace `idVendor` and `idProduct` with your hub's IDs.

### Hub not found

**Error:**
```
Error -32100: Hub not found: 2109:2817
```

**Solution:**
1. Run `hub discover` to refresh the hub list
2. Verify the hub is connected: `uhubctl`
3. Check the hub ID is correct

### Port already reserved

**Error:**
```
Error -32102: Port already reserved by: master
```

**Solution:**
1. Check who reserved the port: `hub status <hub_id>`
2. Wait for the reservation to expire
3. Or release the port: `hub release <hub_id> <port> --role master`

## Best Practices

1. **Always use reservations** when testing to avoid conflicts
2. **Set reasonable durations** for reservations (5-10 minutes)
3. **Power off unused ports** to save power and prevent interference
4. **Clean up reservations** after tests complete
5. **Use role-based access** to track which device is using which port
6. **Discover hubs at startup** to get the latest hub status

## Limitations

1. **uhubctl required** - Must be installed on the remote device
2. **Hub compatibility** - Not all USB hubs support per-port power control
3. **Linux only** - Currently only supports Linux systems
4. **No automatic conflict resolution** - Manual coordination required
5. **No persistent state** - Reservations are lost on restart

## Future Enhancements

Potential future improvements:

- [ ] Persistent reservation storage
- [ ] Automatic reservation expiry cleanup
- [ ] Hub hotplug detection
- [ ] USB device enumeration and tracking
- [ ] Conflict resolution policies
- [ ] Hub configuration files
- [ ] Multi-hub orchestration
- [ ] Windows/macOS support
