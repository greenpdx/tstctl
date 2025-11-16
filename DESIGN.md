# Distributed Network Test Framework Design

## Overview

A comprehensive test framework for backend network functionality testing with distributed architecture:

- **Master**: Coordinates test execution, collects results, generates reports
- **UUT Client**: Runs on the router (Unit Under Test), controls device state
- **Slave Clients**: Run on test machines, perform network actions against UUT

**Location**: `/systests/` directory (backend testing only)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         MASTER                              │
│  - Test orchestration                                       │
│  - Result aggregation                                       │
│  - Report generation                                        │
│  - USB sideband communication (optional)                    │
└─────────┬───────────────────────────────────┬───────────────┘
          │                                   │
          │ Control Channel                   │ Control Channel
          │ (USB sideband or network)        │ (network or USB)
          │                                   │
          ▼                                   ▼
┌─────────────────────┐            ┌──────────────────────────┐
│    UUT CLIENT       │◄───────────┤   SLAVE CLIENT(S)        │
│  - Router control   │  Network   │  - WiFi 2.4 GHz          │
│  - State setup      │  Actions   │  - WiFi 5 GHz            │
│  - Metric collection│            │  - Ethernet              │
│  - Plugin control   │            │  - Network operations    │
└─────────────────────┘            └──────────────────────────┘
         │                                    │
         │                                    │
         └────────────TEST ACTIONS────────────┘
         (DHCP, DNS, routing, ACL, filtering, etc.)
```

---

## Communication Channels

### Primary: Network-based Control
- **Protocol**: JSON-RPC 2.0 over TCP/WebSocket
- **Port**: Configurable (default 9000 for master)
- **Advantages**: Simple, no hardware dependencies
- **Limitations**: Requires working network, affected by UUT state

### Secondary: USB Sideband (Optional)
- **Protocol**: Serial over USB or USB network gadget
- **Use Cases**:
  - Out-of-band control when network is down
  - Coordinating network reconfigurations
  - Emergency recovery/reset
- **Implementation Options**:
  1. **USB Serial** (`/dev/ttyUSB*`, `/dev/ttyACM*`)
  2. **USB Ethernet Gadget** (UUT acts as USB ethernet device)
  3. **USB Mass Storage** (message passing via filesystem)

---

## Virtual Port Strategy (Addressing Hardware Limitations)

### Problem
Each slave has limited physical interfaces:
- 1× WiFi 2.4 GHz
- 1× WiFi 5 GHz
- 1× Ethernet

### Solutions

#### 1. VLAN Tagging (802.1Q)
Create multiple virtual networks on single ethernet port:
```
Slave Ethernet Port
  ├── eth0.10 (VLAN 10) - Management
  ├── eth0.20 (VLAN 20) - Test Network A
  ├── eth0.30 (VLAN 30) - Test Network B
  └── eth0.40 (VLAN 40) - Test Network C
```

**Use Cases**: Multi-subnet routing tests, ACL across VLANs

#### 2. WiFi Virtual Interfaces (mac80211)
Create multiple virtual WiFi interfaces:
```
wlan0 (physical)
  ├── wlan0_sta1 (Station mode, SSID: Test-2G)
  ├── wlan0_sta2 (Station mode, SSID: Test-2G-Guest)
  └── wlan0_mon (Monitor mode for packet inspection)
```

**Use Cases**: Multiple client simulation, guest network isolation testing

#### 3. Network Namespaces
Isolate network stacks for parallel testing:
```
Host Network Namespace
  ├── ns_client1 (eth0.10, isolated routing table)
  ├── ns_client2 (eth0.20, isolated routing table)
  └── ns_client3 (wlan0_sta1, isolated routing table)
```

**Use Cases**: Parallel test execution, routing isolation

#### 4. Bridge Interfaces
Combine interfaces or connect namespaces:
```
br-test0
  ├── eth0.10
  ├── veth-ns1
  └── veth-ns2
```

**Use Cases**: Complex topology testing

---

## Component Design

### 1. Master Controller (`systests/master/`)

**Responsibilities**:
- Load test definitions from YAML/TOML
- Coordinate UUT and slave clients
- Execute test sequences
- Collect and aggregate results
- Generate reports (JSON, HTML, JUnit XML)
- Handle test failures and retries

**Structure**:
```rust
systests/master/
├── src/
│   ├── main.rs              // CLI entry point
│   ├── orchestrator.rs      // Test execution coordination
│   ├── scheduler.rs         // Parallel/sequential test scheduling
│   ├── reporter.rs          // Result aggregation and reporting
│   ├── client_manager.rs    // UUT/slave client connection management
│   ├── sideband/
│   │   ├── mod.rs           // USB sideband abstraction
│   │   ├── serial.rs        // USB serial implementation
│   │   └── usb_gadget.rs    // USB ethernet gadget implementation
│   └── transport/
│       ├── mod.rs           // Transport abstraction
│       ├── tcp.rs           // TCP transport
│       └── websocket.rs     // WebSocket transport
├── tests/                   // Test definitions (YAML/TOML)
│   ├── dhcp/
│   │   ├── basic_lease.yaml
│   │   └── renewal.yaml
│   ├── dns/
│   │   ├── resolution.yaml
│   │   └── forwarding.yaml
│   ├── firewall/
│   │   ├── port_filtering.yaml
│   │   └── acl_rules.yaml
│   └── routing/
│       └── multi_subnet.yaml
└── Cargo.toml
```

**Test Definition Format** (YAML):
```yaml
name: "DHCP Basic Lease Acquisition"
description: "Verify slave can obtain IP via DHCP from UUT"
timeout: 30s
retry: 3

setup:
  uut:
    - action: configure_dhcp
      params:
        interface: "br-lan"
        pool: "192.168.1.100-192.168.1.200"
        lease_time: "1h"
  slaves:
    - id: "slave1"
      actions:
        - action: set_interface_down
          params:
            interface: "eth0"

execute:
  - slave: "slave1"
    action: request_dhcp
    params:
      interface: "eth0"
      timeout: 10s
    expect:
      ip_assigned: true
      gateway: "192.168.1.1"
      dns_servers: ["192.168.1.1"]

verify:
  uut:
    - action: check_dhcp_leases
      expect:
        active_leases: 1
        client_mac: "${slave1.mac}"
  slaves:
    - id: "slave1"
      action: ping
      params:
        target: "192.168.1.1"
        count: 4
      expect:
        success_rate: 100%

cleanup:
  uut:
    - action: clear_dhcp_leases
  slaves:
    - id: "slave1"
      action: release_dhcp
```

---

### 2. UUT Client (`systests/uut-client/`)

**Responsibilities**:
- Receive commands from master
- Configure router state (network, DHCP, DNS, firewall, etc.)
- Collect metrics and logs
- Report status back to master

**Integration**:
- Uses existing JSON-RPC IPC to communicate with `crrouterd` daemon
- Leverages existing plugin system (DHCP, DNS, firewall, network)
- Minimal code - primarily RPC forwarding + metric collection

**Structure**:
```rust
systests/uut-client/
├── src/
│   ├── main.rs              // Client daemon
│   ├── rpc_bridge.rs        // Bridge to crrouterd (existing IPC)
│   ├── actions/
│   │   ├── mod.rs           // Action dispatcher
│   │   ├── dhcp.rs          // DHCP configuration actions
│   │   ├── dns.rs           // DNS configuration actions
│   │   ├── firewall.rs      // Firewall rule actions
│   │   ├── network.rs       // Network interface actions
│   │   └── system.rs        // System-level actions
│   ├── metrics/
│   │   ├── mod.rs           // Metric collection
│   │   ├── interface.rs     // Interface stats (bytes, packets, errors)
│   │   ├── connections.rs   // Active connections tracking
│   │   └── system.rs        // CPU, memory, load
│   └── transport/
│       ├── mod.rs           // Transport abstraction
│       ├── tcp_server.rs    // TCP listener for master
│       └── sideband.rs      // USB sideband support
└── Cargo.toml
```

**Action Examples**:
```rust
pub enum UutAction {
    // DHCP
    ConfigureDhcp { interface: String, pool: IpRange, lease_time: Duration },
    GetDhcpLeases,
    ClearDhcpLeases,

    // DNS
    ConfigureDns { forwarders: Vec<IpAddr>, zones: Vec<DnsZone> },
    AddDnsRecord { zone: String, record: DnsRecord },

    // Firewall
    AddFirewallRule { rule: NftRule },
    RemoveFirewallRule { handle: u64 },
    FlushFirewallChain { table: String, chain: String },

    // Network
    SetInterfaceState { interface: String, up: bool },
    ConfigureInterface { interface: String, config: InterfaceConfig },
    AddRoute { destination: IpNetwork, gateway: IpAddr },

    // Metrics
    GetInterfaceStats { interface: String },
    GetConnectionTable,
    GetSystemMetrics,
}
```

---

### 3. Slave Client (`systests/slave-client/`)

**Responsibilities**:
- Receive commands from master
- Perform network operations (DHCP, DNS queries, traffic generation, etc.)
- Manage virtual interfaces (VLANs, WiFi virtual interfaces, namespaces)
- Capture packets for verification
- Report results back to master

**Structure**:
```rust
systests/slave-client/
├── src/
│   ├── main.rs              // Client daemon
│   ├── actions/
│   │   ├── mod.rs           // Action dispatcher
│   │   ├── dhcp.rs          // DHCP client operations
│   │   ├── dns.rs           // DNS resolution testing
│   │   ├── ping.rs          // ICMP ping tests
│   │   ├── traffic.rs       // Traffic generation (iperf-like)
│   │   ├── http.rs          // HTTP/HTTPS requests
│   │   └── packet_capture.rs // tcpdump/pcap integration
│   ├── network/
│   │   ├── mod.rs           // Network management
│   │   ├── vlan.rs          // VLAN creation/management
│   │   ├── wifi.rs          // WiFi virtual interface management
│   │   ├── namespace.rs     // Network namespace management
│   │   └── bridge.rs        // Bridge interface management
│   ├── transport/
│   │   ├── mod.rs           // Transport abstraction
│   │   ├── tcp_client.rs    // TCP connection to master
│   │   └── sideband.rs      // USB sideband support
│   └── metrics/
│       └── collector.rs     // Metric collection and reporting
└── Cargo.toml
```

**Action Examples**:
```rust
pub enum SlaveAction {
    // DHCP
    RequestDhcp { interface: String, timeout: Duration },
    ReleaseDhcp { interface: String },
    RenewDhcp { interface: String },

    // DNS
    ResolveHostname { hostname: String, record_type: DnsRecordType },

    // Connectivity
    Ping { target: IpAddr, count: u32, timeout: Duration },
    Traceroute { target: IpAddr, max_hops: u8 },

    // Traffic
    GenerateTraffic { protocol: Protocol, target: IpAddr, port: u16, duration: Duration },
    MeasureThroughput { protocol: Protocol, target: IpAddr, port: u16 },

    // HTTP
    HttpGet { url: String, expect_status: u16 },
    HttpPost { url: String, body: Vec<u8>, expect_status: u16 },

    // Virtual Interfaces
    CreateVlan { parent: String, vlan_id: u16 },
    CreateWifiVirtualInterface { parent: String, name: String },
    CreateNetworkNamespace { name: String },

    // Packet Capture
    StartCapture { interface: String, filter: String },
    StopCapture { capture_id: String },
    GetCapturedPackets { capture_id: String },
}
```

---

### 4. Common Library (`systests/common/`)

Shared code between master, UUT client, and slave clients:

```rust
systests/common/
├── src/
│   ├── lib.rs
│   ├── protocol.rs          // RPC protocol definitions
│   ├── types.rs             // Shared types (IpRange, DnsRecord, etc.)
│   ├── errors.rs            // Error types
│   └── test_result.rs       // Test result structures
└── Cargo.toml
```

**Protocol Definition**:
```rust
#[derive(Serialize, Deserialize)]
pub enum Message {
    // Master → Client
    ExecuteAction { id: String, action: Action, params: Value },
    GetStatus,
    Shutdown,

    // Client → Master
    ActionResult { id: String, result: Result<Value, Error> },
    StatusReport { status: ClientStatus },
    Log { level: LogLevel, message: String },
}

#[derive(Serialize, Deserialize)]
pub enum Action {
    Uut(UutAction),
    Slave(SlaveAction),
}
```

---

## Test Execution Flow

### Example: DHCP Lease Test

```
1. Master loads test definition: tests/dhcp/basic_lease.yaml

2. Master → UUT Client: Configure DHCP server
   UUT Client → crrouterd: ConfigureDhcpServer(pool, lease_time)
   UUT Client → Master: Success

3. Master → Slave1: Bring interface down
   Slave1 → OS: ip link set eth0 down
   Slave1 → Master: Success

4. Master → Slave1: Request DHCP lease
   Slave1 → OS: dhclient eth0
   Slave1 waits for IP assignment
   Slave1 → Master: Success(ip: 192.168.1.101, gateway: 192.168.1.1)

5. Master → UUT Client: Verify lease table
   UUT Client → crrouterd: GetDhcpLeases()
   UUT Client → Master: Success(leases: [...])

6. Master validates results against expectations

7. Master → UUT Client: Clear DHCP leases
   UUT Client → crrouterd: ClearDhcpLeases()

8. Master → Slave1: Release DHCP lease
   Slave1 → OS: dhclient -r eth0

9. Master generates test report
```

---

## Advanced Testing Capabilities

### Multi-Slave Parallel Testing

**VLAN-based Isolation**:
```yaml
name: "Multi-Client DHCP Stress Test"
slaves:
  - id: slave1
    vlan: 10
    expected_subnet: "192.168.10.0/24"
  - id: slave2
    vlan: 20
    expected_subnet: "192.168.20.0/24"
  - id: slave3
    vlan: 30
    expected_subnet: "192.168.30.0/24"

execute:
  parallel: true
  actions:
    - slaves: ["slave1", "slave2", "slave3"]
      action: request_dhcp_loop
      params:
        iterations: 100
        delay: 100ms
```

### WiFi Multi-Interface Testing

```yaml
name: "Guest Network Isolation Test"
setup:
  uut:
    - action: configure_wifi_ssid
      params:
        ssid: "MainNetwork"
        vlan: 1
    - action: configure_wifi_ssid
      params:
        ssid: "GuestNetwork"
        vlan: 99
        isolation: true

  slave1:
    - action: create_wifi_virtual_interfaces
      params:
        interfaces:
          - name: wlan0_main
            ssid: "MainNetwork"
          - name: wlan0_guest
            ssid: "GuestNetwork"

execute:
  - slave: slave1
    action: wifi_connect
    params:
      interface: wlan0_main
      ssid: "MainNetwork"

  - slave: slave1
    action: wifi_connect
    params:
      interface: wlan0_guest
      ssid: "GuestNetwork"

verify:
  - slave: slave1
    action: ping_from_interface
    params:
      interface: wlan0_guest
      target: "${wlan0_main.ip}"  # Try to ping main network from guest
    expect:
      success: false  # Should fail due to isolation
```

### Namespace-based Routing Tests

```yaml
name: "Policy-based Routing Test"
setup:
  slave1:
    - action: create_namespaces
      params:
        namespaces:
          - name: ns_wan1
            interface: eth0.10
          - name: ns_wan2
            interface: eth0.20

execute:
  - slave: slave1
    namespace: ns_wan1
    action: traceroute
    params:
      target: "8.8.8.8"
    expect:
      first_hop: "192.168.10.1"  # Gateway for WAN1

  - slave: slave1
    namespace: ns_wan2
    action: traceroute
    params:
      target: "8.8.8.8"
    expect:
      first_hop: "192.168.20.1"  # Gateway for WAN2
```

---

## USB Sideband Implementation Options

### Option 1: USB Serial (Simple, Reliable)

**UUT Setup** (OpenWrt):
```bash
# Load USB serial gadget
modprobe g_serial

# Creates /dev/ttyGS0 on UUT
# Creates /dev/ttyACM0 on master/slave
```

**Communication**:
- Simple serial protocol (newline-delimited JSON)
- 115200 baud (or higher)
- Fallback when network is unavailable

### Option 2: USB Ethernet Gadget (Higher Bandwidth)

**UUT Setup** (OpenWrt):
```bash
# Load USB ethernet gadget
modprobe g_ether

# UUT gets usb0 interface: 192.168.7.1
# Master/Slave gets usb0: 192.168.7.2
```

**Communication**:
- Full TCP/IP stack over USB
- Higher bandwidth than serial
- Can run parallel to main network testing

### Option 3: USB Mass Storage (Message Passing)

**UUT Setup**:
```bash
# Create a small disk image for message passing
dd if=/dev/zero of=/tmp/messages.img bs=1M count=10
mkfs.vfat /tmp/messages.img
modprobe g_mass_storage file=/tmp/messages.img
```

**Communication**:
- Master/slave write JSON files to mounted USB drive
- UUT polls for new messages
- Lower performance, but no drivers needed

---

## Directory Structure

```
systests/
├── DESIGN.md                    # This document
├── README.md                    # Quick start guide
├── Cargo.toml                   # Workspace configuration
├── common/                      # Shared library
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── protocol.rs
│       ├── types.rs
│       └── errors.rs
├── master/                      # Master controller
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── orchestrator.rs
│   │   ├── scheduler.rs
│   │   ├── reporter.rs
│   │   ├── client_manager.rs
│   │   ├── sideband/
│   │   └── transport/
│   └── tests/                   # Test definitions (YAML)
│       ├── dhcp/
│       ├── dns/
│       ├── firewall/
│       ├── routing/
│       └── wifi/
├── uut-client/                  # UUT client (runs on router)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── rpc_bridge.rs
│       ├── actions/
│       ├── metrics/
│       └── transport/
├── slave-client/                # Slave client (runs on test machines)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── actions/
│       ├── network/
│       ├── transport/
│       └── metrics/
├── scripts/                     # Helper scripts
│   ├── setup-usb-sideband.sh   # Configure USB gadgets
│   ├── setup-vlan.sh           # Configure VLANs
│   ├── setup-wifi-virtual.sh   # Configure WiFi virtual interfaces
│   └── deploy-clients.sh       # Deploy clients to UUT/slaves
└── docs/
    ├── test-writing-guide.md   # How to write tests
    ├── deployment.md           # Deployment instructions
    └── troubleshooting.md      # Common issues
```

---

## Implementation Phases

### Phase 1: Core Framework (MVP)
**Goal**: Basic master-slave communication with simple DHCP test

- [ ] Common library (protocol, types)
- [ ] Master: Basic orchestrator, TCP transport
- [ ] UUT Client: RPC bridge to crrouterd, basic DHCP actions
- [ ] Slave Client: Basic DHCP request action
- [ ] Single test: DHCP lease acquisition
- [ ] Text-based test reporting

**Deliverable**: Working end-to-end test: Slave requests DHCP from UUT, test passes/fails

### Phase 2: Core Network Tests
**Goal**: Complete basic network functionality testing

- [ ] Master: YAML test definitions, parallel execution
- [ ] UUT Client: DNS, firewall, network interface actions
- [ ] Slave Client: DNS resolution, ping, HTTP requests
- [ ] Test suite: DHCP (basic, renewal, release), DNS (resolution, forwarding), Ping (connectivity)
- [ ] HTML test reports

### Phase 3: Virtual Interface Support
**Goal**: Advanced testing with VLANs and virtual interfaces

- [ ] Slave Client: VLAN management
- [ ] Slave Client: WiFi virtual interface support
- [ ] Slave Client: Network namespace support
- [ ] Test suite: Multi-VLAN routing, WiFi isolation, policy routing
- [ ] Master: Topology visualization in reports

### Phase 4: USB Sideband
**Goal**: Out-of-band control channel

- [ ] USB serial transport (master, UUT, slave)
- [ ] USB ethernet gadget transport
- [ ] Automatic failover: network → USB
- [ ] Test suite: Network reconfiguration tests (interface down/up)

### Phase 5: Advanced Testing & Metrics
**Goal**: Production-ready framework

- [ ] Packet capture integration (tcpdump/libpcap)
- [ ] Traffic generation (iperf3-like)
- [ ] Performance metrics (throughput, latency, jitter)
- [ ] Firewall rule verification (port filtering, ACLs)
- [ ] Test suite: Firewall, QoS, traffic shaping, performance
- [ ] CI/CD integration (JUnit XML output)

---

## Technology Stack

### Language
- **Rust** (matches existing codebase)
  - Memory safety for network operations
  - Excellent async support (tokio)
  - Strong serialization (serde)

### Key Dependencies
- **tokio**: Async runtime
- **serde** / **serde_json** / **serde_yaml**: Serialization
- **axum** or **tarpc**: RPC framework (or custom JSON-RPC)
- **nix**: Unix system calls (interfaces, namespaces)
- **pnet**: Packet manipulation
- **pcap**: Packet capture
- **reqwest**: HTTP client (slave)
- **tracing**: Logging
- **serialport**: USB serial communication

### Test Definition Format
- **YAML**: Human-readable, supports comments, easy templating

### Reporting Formats
- **JSON**: Machine-readable, for CI/CD integration
- **HTML**: Human-readable, with charts and visualizations
- **JUnit XML**: CI/CD integration (Jenkins, GitLab CI, etc.)

---

## Key Design Decisions

### 1. Why JSON-RPC over WebSocket/TCP?
- **Compatibility**: Matches existing crrouterd protocol
- **Simplicity**: Easy to debug, extensible
- **Language-agnostic**: Could support non-Rust clients in future

### 2. Why YAML for test definitions?
- **Readability**: Easy for test authors
- **Comments**: Document test intent
- **Templating**: Can use anchors/aliases for reuse

### 3. Why separate UUT and Slave clients?
- **Different roles**: UUT configures, Slave exercises
- **Deployment**: UUT runs on router (constrained), Slave on test machines
- **Security**: UUT has privileged access to crrouterd

### 4. Why support USB sideband?
- **Reliability**: Network-independent control channel
- **Testing**: Can test network failures, reconfigurations
- **Recovery**: Emergency access if network tests break connectivity

### 5. Why virtual interfaces?
- **Cost**: Maximize testing with limited hardware
- **Complexity**: Test advanced scenarios (VLANs, multi-SSID)
- **Isolation**: Prevent test interference

---

## Open Questions for Evaluation

1. **Communication Priority**:
   - Should USB sideband be primary or fallback?
   - Should we support both network + USB simultaneously?

2. **Test Isolation**:
   - Should each test start with a clean UUT state (factory reset)?
   - Or rely on cleanup actions?

3. **Slave Machine OS**:
   - Target Linux only, or support Windows/macOS slaves?
   - Affects virtual interface implementation

4. **Deployment**:
   - How to deploy uut-client to router? (SSH, USB storage, package manager)
   - How to deploy slave-client to test machines?

5. **Concurrency**:
   - Should master support running multiple test suites in parallel?
   - Or strictly sequential execution?

6. **Real-time Monitoring**:
   - Should master provide live test progress dashboard?
   - Or just post-test reports?

7. **Test Data**:
   - Should we support test data fixtures (files, packets)?
   - How to distribute to slaves?

---

## Success Criteria

### MVP Success (Phase 1)
- [ ] Master can load YAML test definition
- [ ] Master can communicate with UUT client over TCP
- [ ] Master can communicate with Slave client over TCP
- [ ] UUT client can configure DHCP via crrouterd
- [ ] Slave client can request DHCP lease
- [ ] Test passes when slave receives correct IP
- [ ] Test report shows pass/fail status

### Full Framework Success (Phase 5)
- [ ] 50+ network tests covering: DHCP, DNS, routing, firewall, WiFi
- [ ] Support for VLANs, WiFi virtual interfaces, network namespaces
- [ ] USB sideband fully functional
- [ ] Packet capture and analysis
- [ ] Performance benchmarking (throughput, latency)
- [ ] HTML reports with charts and visualizations
- [ ] CI/CD integration via JUnit XML
- [ ] Documentation: API reference, test writing guide, deployment guide

---

## Next Steps

Please review this design and provide feedback on:

1. **Architecture**: Does the master-UUT-slave separation make sense?
2. **Virtual interfaces**: Which strategies (VLAN, WiFi virtual, namespaces) are most valuable?
3. **USB sideband**: Which implementation (serial, ethernet, mass storage) fits your setup?
4. **Test priorities**: Which network features should we test first?
5. **Deployment**: How do you envision deploying clients to UUT and slaves?
6. **Open questions**: Answers to the questions in "Open Questions for Evaluation"

Once approved, I'll begin implementation with Phase 1 (Core Framework MVP).
