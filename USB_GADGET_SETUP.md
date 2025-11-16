# USB Gadget Setup Guide

This guide explains how to set up USB gadget networking for TestCtl protocol communication.

## Overview

USB gadget mode allows devices to act as USB peripherals, presenting themselves as network interfaces to a host computer. This enables direct communication over USB without requiring Ethernet cables or WiFi.

```
┌──────────────┐         USB Cable         ┌──────────────┐
│    Master    │◄──────────────────────────►│     UUT      │
│  (USB Host)  │                            │ (USB Gadget) │
│              │                            │              │
│ 192.168.100.1│                            │192.168.100.2 │
└──────────────┘                            └──────────────┘
```

## Prerequisites

### Master Side (Test Controller)

**Hardware Options:**
- Linux PC with USB host controller
- Raspberry Pi (any model)
- Any Linux system with USB ports

**Software:**
- Linux kernel with USB networking support
- `ip` command (iproute2 package)
- TestCtl master binary

### Remote Side (UUT/Slave - OpenWrt Device)

**Hardware Requirements:**
- Device with USB OTG/gadget support
- Most modern OpenWrt-capable devices support this

**Software:**
- OpenWrt with USB gadget kernel modules
- TestCtl remote binary

## Linux Kernel Requirements

### Required Kernel Modules

**On Remote (OpenWrt):**
```bash
# Load USB gadget modules
modprobe libcomposite
modprobe u_ether
modprobe g_ether

# Or for RNDIS (Windows/Linux compatible)
modprobe usb_f_rndis
modprobe usb_f_ecm
```

**On Master (Linux PC):**
```bash
# Usually already loaded, but if needed:
modprobe cdc_ether
modprobe rndis_host
```

### Check Kernel Support

```bash
# Check for USB gadget support
ls /sys/kernel/config/usb_gadget 2>/dev/null && echo "USB gadget supported" || echo "Not supported"

# Check for configfs
mount | grep configfs
# Should show: configfs on /sys/kernel/config type configfs

# If not mounted:
sudo mount -t configfs none /sys/kernel/config
```

## Automatic Setup (Recommended)

TestCtl can automatically configure USB gadget networking.

### On Remote (OpenWrt Device)

```bash
# One-time setup (creates USB gadget device)
sudo testctl-remote --setup-gadget --role uut

# This will:
# 1. Mount configfs if needed
# 2. Create USB gadget device
# 3. Configure RNDIS function
# 4. Set up network interface
# 5. Assign IP address 192.168.100.2/24
```

### On Master

```bash
# TestCtl master will automatically configure the interface
# when using USB mode (default)
testctl-master ping

# Or manually configure before first use:
sudo ip addr add 192.168.100.1/24 dev usb0
sudo ip link set usb0 up
```

## Manual Setup (Advanced)

### Remote Side (OpenWrt)

#### 1. Mount ConfigFS

```bash
sudo mkdir -p /sys/kernel/config
sudo mount -t configfs none /sys/kernel/config
```

#### 2. Create USB Gadget

```bash
cd /sys/kernel/config/usb_gadget
sudo mkdir testctl
cd testctl

# Set USB IDs
echo 0x1d6b > idVendor  # Linux Foundation
echo 0x0104 > idProduct # Multifunction Composite Gadget

# Set device info
sudo mkdir strings/0x409
echo "testctl-001" > strings/0x409/serialnumber
echo "TestCtl" > strings/0x409/manufacturer
echo "TestCtl Network" > strings/0x409/product
```

#### 3. Create Configuration

```bash
sudo mkdir configs/c.1
sudo mkdir configs/c.1/strings/0x409
echo "RNDIS" > configs/c.1/strings/0x409/configuration
```

#### 4. Create RNDIS Function

```bash
sudo mkdir functions/rndis.usb0

# Optional: Set MAC addresses
echo "02:00:00:00:00:01" > functions/rndis.usb0/host_addr
echo "02:00:00:00:00:02" > functions/rndis.usb0/dev_addr
```

#### 5. Link Function to Configuration

```bash
sudo ln -s functions/rndis.usb0 configs/c.1/
```

#### 6. Enable Gadget

```bash
# Find available UDC (USB Device Controller)
ls /sys/class/udc
# Example output: ci_hdrc.0

# Bind to UDC
echo "ci_hdrc.0" > UDC  # Replace with your UDC name
```

#### 7. Configure Network Interface

The USB gadget will create a network interface (usually `usb0`):

```bash
sudo ip addr add 192.168.100.2/24 dev usb0
sudo ip link set usb0 up

# Verify
ip addr show usb0
```

### Master Side (Linux PC)

When you connect the USB cable, the system should detect a new network interface:

```bash
# Wait for interface to appear (usually usb0)
dmesg | tail
# Should show: usb0: register 'rndis_host'

# Configure interface
sudo ip addr add 192.168.100.1/24 dev usb0
sudo ip link set usb0 up

# Test connectivity
ping 192.168.100.2
```

## NetworkManager Configuration

If using NetworkManager, it may interfere with manual configuration.

### Option 1: Disable NetworkManager for usb0

Create `/etc/NetworkManager/conf.d/usb-gadget.conf`:

```ini
[keyfile]
unmanaged-devices=interface-name:usb0
```

Reload NetworkManager:
```bash
sudo systemctl reload NetworkManager
```

### Option 2: Use NetworkManager

Configure via GUI or nmcli:
```bash
sudo nmcli connection add type ethernet ifname usb0 con-name testctl \
  ipv4.method manual \
  ipv4.addresses 192.168.100.1/24
sudo nmcli connection up testctl
```

## OpenWrt Persistence

To make USB gadget configuration persistent on OpenWrt:

### Create Init Script

Create `/etc/init.d/testctl-gadget`:

```bash
#!/bin/sh /etc/rc.common

START=90
STOP=10

USE_PROCD=1

start_service() {
    # Load modules
    modprobe libcomposite

    # Mount configfs
    [ -d /sys/kernel/config/usb_gadget ] || \
        mount -t configfs none /sys/kernel/config

    # Run setup
    testctl-remote --setup-gadget --role uut &
}

stop_service() {
    killall testctl-remote
}
```

Enable on boot:
```bash
chmod +x /etc/init.d/testctl-gadget
/etc/init.d/testctl-gadget enable
/etc/init.d/testctl-gadget start
```

## Troubleshooting

### Interface Not Appearing

**On Remote:**
```bash
# Check UDC binding
cat /sys/kernel/config/usb_gadget/testctl/UDC
# Should show UDC name, not empty

# Check kernel messages
dmesg | grep -i usb
```

**On Master:**
```bash
# Check USB connection
lsusb
# Should show device with VID:PID 1d6b:0104

# Check kernel messages
dmesg | tail -50 | grep -i usb
```

### Cannot Ping

```bash
# Check interface is up
ip link show usb0
# Should show: state UP

# Check IP addresses
ip addr show usb0
# Should show configured IP

# Check routing
ip route show

# Test with tcpdump
sudo tcpdump -i usb0 icmp
# Then ping from other side
```

### Permission Denied

USB gadget setup requires root:
```bash
sudo testctl-remote --setup-gadget
```

### Module Not Found

Install required packages on OpenWrt:
```bash
opkg update
opkg install kmod-usb-gadget kmod-usb-gadget-eth
```

## Testing

### Quick Test

**Terminal 1 (Remote):**
```bash
sudo testctl-remote --setup-gadget --tcp --tcp-addr 192.168.100.2:9999
```

**Terminal 2 (Master):**
```bash
# Configure interface
sudo ip addr add 192.168.100.1/24 dev usb0
sudo ip link set usb0 up

# Test connectivity
ping 192.168.100.2

# Test protocol
testctl-master --tcp --remote-addr 192.168.100.2:9999 ping
```

## Multiple Devices

You can connect multiple devices using different USB interfaces:

```
Master (192.168.100.1)
  ├── usb0 → UUT   (192.168.100.2)
  └── usb1 → Slave (192.168.101.2)
```

Configure different IP ranges for each:
```bash
# UUT on usb0
sudo ip addr add 192.168.100.1/24 dev usb0
sudo ip link set usb0 up

# Slave on usb1
sudo ip addr add 192.168.101.1/24 dev usb1
sudo ip link set usb1 up
```

## References

- [Linux USB Gadget API](https://www.kernel.org/doc/html/latest/usb/gadget.html)
- [ConfigFS USB Gadget](https://www.kernel.org/doc/html/latest/usb/gadget_configfs.html)
- [OpenWrt USB Configuration](https://openwrt.org/docs/guide-user/hardware/usb)
