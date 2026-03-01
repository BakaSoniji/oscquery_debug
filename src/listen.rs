use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use crossterm::{cursor, execute, style, terminal};
use mdns_sd::ServiceDaemon;
use rosc::OscPacket;

use crate::osc;
use crate::protocol::{self, MDNS_OSC_UDP, MDNS_OSCJSON_TCP};
use crate::report;

// ─── Interface selection ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct NetworkInterface {
    name: String,
    ip: Ipv4Addr,
}

fn enumerate_interfaces() -> Vec<NetworkInterface> {
    let Ok(addrs) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    addrs
        .into_iter()
        .filter(|interface| !interface.is_loopback())
        .filter_map(|interface| {
            if let IpAddr::V4(ip) = interface.ip() {
                Some(NetworkInterface {
                    name: interface.name,
                    ip,
                })
            } else {
                None
            }
        })
        .collect()
}

fn select_interface(requested: Option<&str>) -> Result<NetworkInterface> {
    if let Some(ip_str) = requested {
        let ip: Ipv4Addr = ip_str
            .parse()
            .with_context(|| format!("invalid interface IP: {ip_str}"))?;
        return Ok(NetworkInterface {
            name: "user-specified".to_string(),
            ip,
        });
    }

    let interfaces = enumerate_interfaces();

    match interfaces.len() {
        0 => bail!("no non-loopback IPv4 network interfaces found"),
        1 => {
            let interface = &interfaces[0];
            report::info(format!("Using interface: {} ({})", interface.name, interface.ip));
            Ok(interface.clone())
        }
        _ => {
            println!("Multiple network interfaces detected:");
            for (i, interface) in interfaces.iter().enumerate() {
                println!("  {}) {:<12} {}", i + 1, interface.name, interface.ip);
            }
            print!(
                "\nWhich interface should we advertise on? [1-{}]: ",
                interfaces.len()
            );
            std::io::stdout().flush()?;

            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let choice: usize = input
                .trim()
                .parse()
                .with_context(|| format!("invalid choice: {}", input.trim()))?;
            if choice < 1 || choice > interfaces.len() {
                bail!("choice out of range: {}", choice);
            }
            Ok(interfaces[choice - 1].clone())
        }
    }
}

// ─── Live TUI display ────────────────────────────────────────────────────────

struct TrackerState {
    position: [f32; 3],
    rotation: [f32; 3],
    last_update: Instant,
}

struct ListenDisplay {
    head: Option<TrackerState>,
    left: Option<TrackerState>,
    right: Option<TrackerState>,
    status_lines: u16,
}

impl ListenDisplay {
    fn new() -> Self {
        Self {
            head: None,
            left: None,
            right: None,
            status_lines: 4,
        }
    }

    fn log(&self, msg: &str) {
        let mut stdout = std::io::stdout();
        execute!(
            stdout,
            cursor::SavePosition,
            cursor::MoveTo(
                0,
                terminal::size()
                    .map(|(_, h)| h.saturating_sub(self.status_lines + 1))
                    .unwrap_or(0)
            ),
            terminal::ScrollUp(1),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::Print(msg),
            cursor::RestorePosition,
        )
        .ok();
        self.redraw_status();
    }

    fn update_tracker(&mut self, which: &str, position: [f32; 3], rotation: [f32; 3]) {
        let state = TrackerState {
            position,
            rotation,
            last_update: Instant::now(),
        };
        match which {
            "head" => self.head = Some(state),
            "leftwrist" => self.left = Some(state),
            "rightwrist" => self.right = Some(state),
            _ => {}
        }
        self.redraw_status();
    }

    fn redraw_status(&self) {
        let mut stdout = std::io::stdout();
        let (width, height) = terminal::size().unwrap_or((80, 24));
        let status_start = height.saturating_sub(self.status_lines);

        execute!(
            stdout,
            cursor::MoveTo(0, status_start),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::Print("\u{2500}".repeat(width as usize)),
        )
        .ok();

        let labels = [
            ("HEAD ", &self.head),
            ("LEFT ", &self.left),
            ("RIGHT", &self.right),
        ];
        for (i, (label, state)) in labels.iter().enumerate() {
            execute!(
                stdout,
                cursor::MoveTo(0, status_start + 1 + i as u16),
                terminal::Clear(terminal::ClearType::CurrentLine),
            )
            .ok();

            match state {
                Some(s) => {
                    let ago = s.last_update.elapsed().as_millis();
                    let ago_str = if ago < 1000 {
                        format!("{}ms ago", ago)
                    } else {
                        format!("{:.1}s ago", ago as f64 / 1000.0)
                    };
                    execute!(
                        stdout,
                        style::Print(format!(
                            " {}  pos=({:>8.3}, {:>8.3}, {:>8.3}) rot=({:>7.1}, {:>7.1}, {:>7.1})  {}",
                            label,
                            s.position[0], s.position[1], s.position[2],
                            s.rotation[0], s.rotation[1], s.rotation[2],
                            ago_str,
                        )),
                    )
                    .ok();
                }
                None => {
                    execute!(
                        stdout,
                        style::Print(format!(
                            " {}  pos=(      --,       --,       --) rot=(     --,      --,      --)  waiting...",
                            label,
                        )),
                    )
                    .ok();
                }
            }
        }

        stdout.flush().ok();
    }

    fn setup_scroll_region(&self) {
        let mut stdout = std::io::stdout();
        let (_, height) = terminal::size().unwrap_or((80, 24));
        execute!(
            stdout,
            terminal::SetSize(terminal::size().unwrap_or((80, 24)).0, height),
        )
        .ok();
        for _ in 0..self.status_lines + 2 {
            println!();
        }
        self.redraw_status();
    }
}

// ─── OSC packet dispatch ────────────────────────────────────────────────────

fn process_osc_packet(packet: &OscPacket, display: &mut ListenDisplay) {
    match packet {
        OscPacket::Message(msg) => {
            if let Some((part, pos, rot)) = osc::parse_vrsystem_osc(msg) {
                display.update_tracker(part, pos, rot);
            } else {
                display.log(&format!(
                    "[OSC]  {} args={}",
                    msg.addr,
                    osc::format_osc_args(&msg.args),
                ));
            }
        }
        OscPacket::Bundle(bundle) => {
            for p in &bundle.content {
                process_osc_packet(p, display);
            }
        }
    }
}

// ─── Main listen entrypoint ──────────────────────────────────────────────────

pub fn run_listen(osc_port: u16, interface: Option<String>) -> Result<()> {
    println!("=== OSCQuery Listen Mode (SlimeVR Stub) ===");
    println!();

    // Step 1: Select interface
    let interface = select_interface(interface.as_deref())?;
    let ip_str = interface.ip.to_string();

    // Step 2: Bind OSC UDP socket
    let osc_addr: SocketAddr = format!("{}:{}", interface.ip, osc_port).parse()?;
    let udp_socket = UdpSocket::bind(osc_addr).with_context(|| {
        format!("failed to bind UDP socket on {osc_addr} -- is SlimeVR still running?")
    })?;
    udp_socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    report::info(format!("OSC UDP listening on {}", osc_addr));

    // Step 3: Start HTTP server for OSCQuery
    let http_addr = format!("{}:0", interface.ip);
    let http_server = tiny_http::Server::http(&http_addr)
        .map_err(|e| anyhow::anyhow!("failed to start HTTP server on {}: {}", http_addr, e))?;
    let http_port = http_server
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .unwrap_or(0);
    let service_name = format!("SlimeVR-Server-{}", http_port);
    report::info(format!(
        "OSCQuery HTTP serving on {}:{}",
        interface.ip, http_port
    ));

    // Step 4: Register mDNS services
    let mdns = ServiceDaemon::new().context("unable to start mDNS daemon")?;
    let hostname = format!("{}.local.", service_name);
    protocol::register_mdns_service(
        &mdns,
        MDNS_OSCJSON_TCP,
        &service_name,
        &hostname,
        interface.ip,
        http_port,
    )?;
    protocol::register_mdns_service(
        &mdns,
        MDNS_OSC_UDP,
        &service_name,
        &hostname,
        interface.ip,
        osc_port,
    )?;
    report::info(format!("mDNS services registered as \"{}\"", service_name));
    report::info(format!(
        "Advertised: _oscjson._tcp on port {}, _osc._udp on port {}",
        http_port, osc_port
    ));

    // Step 5: Set up Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let running_ctrlc = running.clone();
    ctrlc::set_handler(move || {
        running_ctrlc.store(false, Ordering::SeqCst);
    })
    .context("failed to set Ctrl+C handler")?;

    println!();
    report::info("Waiting for VRChat to send OSC data... (Ctrl+C to stop)");
    println!();

    // Step 6: Spawn HTTP server thread
    let host_info = protocol::build_host_info(&service_name, &ip_str, osc_port);
    let host_info_json =
        serde_json::to_string(&host_info).context("failed to serialize HOST_INFO")?;
    let tree_json = protocol::oscquery_tree_json();
    let http_running = running.clone();
    let http_handle = std::thread::spawn(move || {
        while http_running.load(Ordering::SeqCst) {
            match http_server.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(request)) => {
                    let url = request.url().to_string();
                    let response_body = if url.contains("HOST_INFO") {
                        host_info_json.clone()
                    } else {
                        tree_json.clone()
                    };
                    let response = tiny_http::Response::from_string(&response_body).with_header(
                        tiny_http::Header::from_bytes(
                            &b"Content-Type"[..],
                            &b"application/json"[..],
                        )
                        .unwrap(),
                    );
                    request.respond(response).ok();
                }
                Ok(None) => {}
                Err(e) => {
                    report::error(format!("HTTP server thread exiting: {e}"));
                    http_running.store(false, Ordering::SeqCst);
                    break;
                }
            }
        }
    });

    // Step 7: Set up crossterm display
    let mut display = ListenDisplay::new();
    display.setup_scroll_region();

    // Step 8: Main OSC receive loop
    let mut buf = [0u8; 65536];
    let mut last_status_redraw = Instant::now();
    while running.load(Ordering::SeqCst) {
        match udp_socket.recv_from(&mut buf) {
            Ok((size, _src)) => match rosc::decoder::decode_udp(&buf[..size]) {
                Ok((_, packet)) => {
                    process_osc_packet(&packet, &mut display);
                }
                Err(e) => {
                    display.log(&format!("[WARN] Failed to decode OSC packet: {e}"));
                }
            },
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // Throttle idle redraws to once per second (for "ago" counters)
                if last_status_redraw.elapsed() >= Duration::from_secs(1) {
                    display.redraw_status();
                    last_status_redraw = Instant::now();
                }
            }
            Err(e) => {
                display.log(&format!("[ERR]  UDP recv error: {e}"));
                running.store(false, Ordering::SeqCst);
                break;
            }
        }
    }

    // Cleanup
    println!();
    println!();
    report::info("Shutting down...");
    http_handle.join().ok();
    mdns.shutdown().ok();
    report::info("mDNS unregistered. Goodbye.");

    Ok(())
}
