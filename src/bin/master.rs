// Test Control Protocol - Master (Test Coordinator)
//
// Coordinates tests across UUT and slave devices.
// Sends commands via USB gadget networking.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{BufReader, BufWriter};
use tracing::{error, info};

use testctl::{
    read_message, write_message, Message, Method, NetworkConfigureParams, ResponseResult,
    SetRoleParams, TestStartParams, Transport, UsbGadgetConfig, setup_usb_gadget,
    HubDeviceRole, HubPortPowerOnParams, HubPortPowerOffParams, HubPortReserveParams,
    HubPortReleaseParams, HubStatusParams,
};

#[derive(Parser, Debug)]
#[command(author, version, about = "Test Control Protocol Master", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Use TCP instead of USB gadget (for testing)
    #[arg(long, global = true)]
    tcp: bool,

    /// Remote address (if --tcp is used)
    #[arg(long, global = true, default_value = "192.168.100.2:9999")]
    remote_addr: String,

    /// USB gadget interface
    #[arg(short = 'i', long, global = true, default_value = "usb0")]
    interface: String,

    /// Timeout for commands in seconds
    #[arg(short = 't', long, global = true, default_value = "30")]
    timeout: u64,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Ping remote device
    Ping,

    /// Get device information
    Info,

    /// Set device role
    SetRole {
        /// Role (uut or slave)
        role: String,
    },

    /// Start a test
    TestStart {
        /// Test suite name
        suite: String,
        /// Test case name (optional)
        #[arg(short, long)]
        test_case: Option<String>,
        /// Timeout in seconds
        #[arg(short = 't', long)]
        timeout: Option<u64>,
    },

    /// Get test results
    TestResults {
        /// Test ID
        test_id: String,
        /// Include detailed logs
        #[arg(short, long)]
        logs: bool,
    },

    /// Abort a running test
    TestAbort {
        /// Test ID
        test_id: String,
        /// Reason for abort
        #[arg(short, long)]
        reason: Option<String>,
    },

    /// Configure network interface
    NetworkConfigure {
        /// Interface name
        interface: String,
        /// IP address
        ip: String,
        /// Netmask (CIDR)
        netmask: u8,
        /// Gateway (optional)
        #[arg(short, long)]
        gateway: Option<String>,
    },

    /// Get network status
    NetworkStatus {
        /// Interface name
        #[arg(default_value = "usb0")]
        interface: String,
    },

    /// USB Hub commands
    #[command(subcommand)]
    Hub(HubCommands),

    /// Run interactive test session
    Interactive,
}

#[derive(Subcommand, Debug)]
enum HubCommands {
    /// List all USB hubs
    List,

    /// Discover and refresh hub list
    Discover,

    /// Get status of a specific hub
    Status {
        /// Hub ID
        hub_id: String,
    },

    /// Power on a hub port
    PowerOn {
        /// Hub ID
        hub_id: String,
        /// Port number
        port: u8,
    },

    /// Power off a hub port
    PowerOff {
        /// Hub ID
        hub_id: String,
        /// Port number
        port: u8,
    },

    /// Reserve a hub port
    Reserve {
        /// Hub ID
        hub_id: String,
        /// Port number
        port: u8,
        /// Requester role (master, uut, slave)
        #[arg(short, long, default_value = "master")]
        role: String,
        /// Duration in seconds (optional)
        #[arg(short, long)]
        duration: Option<u64>,
        /// Also power on the port
        #[arg(short, long)]
        power_on: bool,
    },

    /// Release a hub port reservation
    Release {
        /// Hub ID
        hub_id: String,
        /// Port number
        port: u8,
        /// Releaser role (master, uut, slave)
        #[arg(short, long, default_value = "master")]
        role: String,
        /// Also power off the port
        #[arg(short, long)]
        power_off: bool,
    },
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

    // Connect to remote
    let transport = if args.tcp {
        let addr: SocketAddr = args.remote_addr.parse().context("Invalid remote address")?;
        info!("Connecting to remote via TCP: {}", addr);
        Transport::connect_tcp(addr).await?
    } else {
        // Setup USB gadget networking first
        let config = UsbGadgetConfig::master();
        info!("Setting up USB gadget on {}", config.interface);
        setup_usb_gadget(&config)?;

        info!("Connecting to remote via USB gadget: {}", config.peer_ip);
        Transport::connect_usb_gadget(&config).await?
    };

    info!("Connected to remote");

    // Execute command
    match execute_command(transport, args.command, args.timeout).await {
        Ok(()) => {
            info!("Command completed successfully");
            Ok(())
        }
        Err(e) => {
            error!("Command failed: {}", e);
            Err(e)
        }
    }
}

/// Execute a command
async fn execute_command(transport: Transport, command: Commands, timeout: u64) -> Result<()> {
    let (read, write) = transport.split();
    let mut reader = BufReader::new(read);
    let mut writer = BufWriter::new(write);

    match command {
        Commands::Ping => {
            let msg = Message::request(Method::Ping, serde_json::json!({}));
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::Info => {
            let msg = Message::request(Method::GetInfo, serde_json::json!({}));
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::SetRole { role } => {
            let role = match role.to_lowercase().as_str() {
                "uut" => testctl::DeviceRole::Uut,
                "slave" => testctl::DeviceRole::Slave,
                _ => anyhow::bail!("Invalid role: {}. Must be 'uut' or 'slave'", role),
            };

            let params = SetRoleParams { role };
            let msg = Message::request(Method::SetRole, serde_json::to_value(params)?);
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::TestStart {
            suite,
            test_case,
            timeout: test_timeout,
        } => {
            let params = TestStartParams {
                suite,
                test_case,
                config: None,
                timeout: test_timeout,
            };
            let msg = Message::request(Method::TestStart, serde_json::to_value(params)?);
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::TestResults { test_id, logs } => {
            let params = testctl::TestResultsParams {
                test_id,
                include_logs: logs,
            };
            let msg = Message::request(Method::TestResults, serde_json::to_value(params)?);
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::TestAbort { test_id, reason } => {
            let params = testctl::TestAbortParams { test_id, reason };
            let msg = Message::request(Method::TestAbort, serde_json::to_value(params)?);
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::NetworkConfigure {
            interface,
            ip,
            netmask,
            gateway,
        } => {
            let ip = ip.parse().context("Invalid IP address")?;
            let gateway = gateway.map(|g| g.parse()).transpose()?;

            let params = NetworkConfigureParams {
                interface,
                ip,
                netmask,
                gateway,
            };
            let msg = Message::request(Method::NetworkConfigure, serde_json::to_value(params)?);
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::NetworkStatus { interface } => {
            let msg = Message::request(
                Method::NetworkStatus,
                serde_json::json!({ "interface": interface }),
            );
            send_request_and_wait(&mut reader, &mut writer, msg, timeout).await?;
        }

        Commands::Hub(hub_cmd) => {
            execute_hub_command(&mut reader, &mut writer, hub_cmd, timeout).await?;
        }

        Commands::Interactive => {
            interactive_session(&mut reader, &mut writer, timeout).await?;
        }
    }

    Ok(())
}

/// Execute a hub command
async fn execute_hub_command<R, W>(
    reader: &mut BufReader<R>,
    writer: &mut BufWriter<W>,
    command: HubCommands,
    timeout: u64,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    match command {
        HubCommands::List => {
            let msg = Message::request(Method::HubList, serde_json::json!({}));
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::Discover => {
            let msg = Message::request(Method::HubDiscover, serde_json::json!({}));
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::Status { hub_id } => {
            let params = HubStatusParams { hub_id };
            let msg = Message::request(Method::HubStatus, serde_json::to_value(params)?);
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::PowerOn { hub_id, port } => {
            let params = HubPortPowerOnParams { hub_id, port };
            let msg = Message::request(Method::HubPortPowerOn, serde_json::to_value(params)?);
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::PowerOff { hub_id, port } => {
            let params = HubPortPowerOffParams { hub_id, port };
            let msg = Message::request(Method::HubPortPowerOff, serde_json::to_value(params)?);
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::Reserve {
            hub_id,
            port,
            role,
            duration,
            power_on,
        } => {
            let requester = parse_hub_role(&role)?;
            let params = HubPortReserveParams {
                hub_id,
                port,
                requester,
                duration_secs: duration,
                power_on,
            };
            let msg = Message::request(Method::HubPortReserve, serde_json::to_value(params)?);
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }

        HubCommands::Release {
            hub_id,
            port,
            role,
            power_off,
        } => {
            let releaser = parse_hub_role(&role)?;
            let params = HubPortReleaseParams {
                hub_id,
                port,
                releaser,
                power_off,
            };
            let msg = Message::request(Method::HubPortRelease, serde_json::to_value(params)?);
            send_request_and_wait(reader, writer, msg, timeout).await?;
        }
    }

    Ok(())
}

/// Parse hub device role from string
fn parse_hub_role(role: &str) -> Result<HubDeviceRole> {
    match role.to_lowercase().as_str() {
        "master" => Ok(HubDeviceRole::Master),
        "uut" => Ok(HubDeviceRole::Uut),
        "slave" => Ok(HubDeviceRole::Slave),
        _ => anyhow::bail!("Invalid role: {}. Must be 'master', 'uut', or 'slave'", role),
    }
}

/// Send a request and wait for response
async fn send_request_and_wait<R, W>(
    reader: &mut BufReader<R>,
    writer: &mut BufWriter<W>,
    request: Message,
    timeout_secs: u64,
) -> Result<serde_json::Value>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    // Extract request ID
    let request_id = match &request {
        Message::Request(req) => req.id.clone(),
        _ => anyhow::bail!("Expected request message"),
    };

    // Send request
    write_message(writer, &request).await?;

    // Wait for response with timeout
    let response = tokio::time::timeout(Duration::from_secs(timeout_secs), async {
        loop {
            let msg = read_message(reader).await?;
            match msg {
                Message::Response(resp) if resp.id == request_id => {
                    return Ok::<_, anyhow::Error>(resp);
                }
                Message::Event(evt) => {
                    // Print events as they arrive
                    println!(
                        "[EVENT] {:?} at {}: {}",
                        evt.event,
                        evt.timestamp,
                        serde_json::to_string_pretty(&evt.data)?
                    );
                }
                _ => {
                    // Ignore other messages
                }
            }
        }
    })
    .await
    .context("Request timeout")??;

    // Handle response
    match response.result {
        ResponseResult::Success { result } => {
            println!("{}", serde_json::to_string_pretty(&result)?);
            Ok(result)
        }
        ResponseResult::Error { error } => {
            anyhow::bail!("Error {}: {}", error.code, error.message);
        }
    }
}

/// Interactive session
async fn interactive_session<R, W>(
    reader: &mut BufReader<R>,
    writer: &mut BufWriter<W>,
    timeout: u64,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    println!("=== Test Control Interactive Session ===");
    println!("Commands:");
    println!("  ping                          - Ping remote");
    println!("  info                          - Get device info");
    println!("  start <suite>                 - Start test suite");
    println!("  results <test_id>             - Get test results");
    println!("  abort <test_id>               - Abort test");
    println!("  network <interface>           - Get network status");
    println!("  hub list                      - List USB hubs");
    println!("  hub discover                  - Discover USB hubs");
    println!("  hub status <hub_id>           - Get hub status");
    println!("  hub on <hub_id> <port>        - Power on port");
    println!("  hub off <hub_id> <port>       - Power off port");
    println!("  hub reserve <hub_id> <port>   - Reserve port");
    println!("  hub release <hub_id> <port>   - Release port");
    println!("  quit                          - Exit");
    println!();

    loop {
        print!("> ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        let parts: Vec<&str> = input.trim().split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        match parts[0] {
            "quit" | "exit" => break,

            "ping" => {
                let msg = Message::request(Method::Ping, serde_json::json!({}));
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "info" => {
                let msg = Message::request(Method::GetInfo, serde_json::json!({}));
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "start" => {
                if parts.len() < 2 {
                    println!("Usage: start <suite>");
                    continue;
                }
                let params = TestStartParams {
                    suite: parts[1].to_string(),
                    test_case: None,
                    config: None,
                    timeout: None,
                };
                let msg = Message::request(Method::TestStart, serde_json::to_value(params)?);
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "results" => {
                if parts.len() < 2 {
                    println!("Usage: results <test_id>");
                    continue;
                }
                let params = testctl::TestResultsParams {
                    test_id: parts[1].to_string(),
                    include_logs: true,
                };
                let msg = Message::request(Method::TestResults, serde_json::to_value(params)?);
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "abort" => {
                if parts.len() < 2 {
                    println!("Usage: abort <test_id>");
                    continue;
                }
                let params = testctl::TestAbortParams {
                    test_id: parts[1].to_string(),
                    reason: Some("User requested".to_string()),
                };
                let msg = Message::request(Method::TestAbort, serde_json::to_value(params)?);
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "network" => {
                let interface = parts.get(1).unwrap_or(&"usb0").to_string();
                let msg = Message::request(
                    Method::NetworkStatus,
                    serde_json::json!({ "interface": interface }),
                );
                let _ = send_request_and_wait(reader, writer, msg, timeout).await;
            }

            "hub" => {
                if parts.len() < 2 {
                    println!("Usage: hub <list|discover|status|on|off|reserve|release>");
                    continue;
                }

                match parts[1] {
                    "list" => {
                        let msg = Message::request(Method::HubList, serde_json::json!({}));
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "discover" => {
                        let msg = Message::request(Method::HubDiscover, serde_json::json!({}));
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "status" => {
                        if parts.len() < 3 {
                            println!("Usage: hub status <hub_id>");
                            continue;
                        }
                        let params = HubStatusParams {
                            hub_id: parts[2].to_string(),
                        };
                        let msg = Message::request(Method::HubStatus, serde_json::to_value(params).unwrap());
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "on" => {
                        if parts.len() < 4 {
                            println!("Usage: hub on <hub_id> <port>");
                            continue;
                        }
                        let port = match parts[3].parse::<u8>() {
                            Ok(p) => p,
                            Err(_) => {
                                println!("Invalid port number");
                                continue;
                            }
                        };
                        let params = HubPortPowerOnParams {
                            hub_id: parts[2].to_string(),
                            port,
                        };
                        let msg = Message::request(Method::HubPortPowerOn, serde_json::to_value(params).unwrap());
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "off" => {
                        if parts.len() < 4 {
                            println!("Usage: hub off <hub_id> <port>");
                            continue;
                        }
                        let port = match parts[3].parse::<u8>() {
                            Ok(p) => p,
                            Err(_) => {
                                println!("Invalid port number");
                                continue;
                            }
                        };
                        let params = HubPortPowerOffParams {
                            hub_id: parts[2].to_string(),
                            port,
                        };
                        let msg = Message::request(Method::HubPortPowerOff, serde_json::to_value(params).unwrap());
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "reserve" => {
                        if parts.len() < 4 {
                            println!("Usage: hub reserve <hub_id> <port> [role]");
                            continue;
                        }
                        let port = match parts[3].parse::<u8>() {
                            Ok(p) => p,
                            Err(_) => {
                                println!("Invalid port number");
                                continue;
                            }
                        };
                        let role = parts.get(4).unwrap_or(&"master");
                        let requester = match parse_hub_role(role) {
                            Ok(r) => r,
                            Err(e) => {
                                println!("Invalid role: {}", e);
                                continue;
                            }
                        };
                        let params = HubPortReserveParams {
                            hub_id: parts[2].to_string(),
                            port,
                            requester,
                            duration_secs: None,
                            power_on: false,
                        };
                        let msg = Message::request(Method::HubPortReserve, serde_json::to_value(params).unwrap());
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    "release" => {
                        if parts.len() < 4 {
                            println!("Usage: hub release <hub_id> <port> [role]");
                            continue;
                        }
                        let port = match parts[3].parse::<u8>() {
                            Ok(p) => p,
                            Err(_) => {
                                println!("Invalid port number");
                                continue;
                            }
                        };
                        let role = parts.get(4).unwrap_or(&"master");
                        let releaser = match parse_hub_role(role) {
                            Ok(r) => r,
                            Err(e) => {
                                println!("Invalid role: {}", e);
                                continue;
                            }
                        };
                        let params = HubPortReleaseParams {
                            hub_id: parts[2].to_string(),
                            port,
                            releaser,
                            power_off: false,
                        };
                        let msg = Message::request(Method::HubPortRelease, serde_json::to_value(params).unwrap());
                        let _ = send_request_and_wait(reader, writer, msg, timeout).await;
                    }

                    _ => {
                        println!("Unknown hub command: {}", parts[1]);
                    }
                }
            }

            _ => {
                println!("Unknown command: {}", parts[0]);
            }
        }
    }

    Ok(())
}
