use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(name = "oscquery_debug", version)]
#[command(about = "SlimeVR/VRChat OSCQuery debugger")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Browse mDNS for OSCQuery services.
    Browse {
        /// Browse time in seconds.
        #[arg(default_value_t = 15)]
        seconds: u64,
        /// Optional case-insensitive filter on service instance name.
        #[arg(long = "instance-filter")]
        instance_filter: Option<String>,
    },
    /// Query a specific OSCQuery endpoint (host:port or URL).
    Query { endpoint: String },
    /// Browse then query the first matching OSCQuery service.
    Auto {
        /// Case-insensitive filter on service instance name (e.g. vrchat or slimevr).
        instance_filter: String,
        /// Browse time in seconds.
        #[arg(default_value_t = 15)]
        seconds: u64,
    },
}

#[derive(Debug, Clone)]
struct DiscoveredService {
    service_type: &'static str,
    instance: String,
    hostname: String,
    port: u16,
    ipv4: Vec<String>,
    txt: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HostInfo {
    #[serde(rename = "NAME")]
    name: Option<String>,
    #[serde(rename = "OSC_IP")]
    osc_ip: Option<String>,
    #[serde(rename = "OSC_PORT")]
    osc_port: Option<u16>,
    #[serde(rename = "OSC_TRANSPORT")]
    osc_transport: Option<String>,
    #[serde(rename = "EXTENSIONS")]
    extensions: Option<BTreeMap<String, bool>>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Browse {
            seconds,
            instance_filter,
        } => {
            let services = browse_services(Duration::from_secs(seconds))?;
            print_browse(&services, instance_filter.as_deref());
        }
        Commands::Query { endpoint } => {
            run_query(&endpoint)?;
        }
        Commands::Auto {
            instance_filter,
            seconds,
        } => {
            run_auto(&instance_filter, Duration::from_secs(seconds))?;
        }
    }
    Ok(())
}

fn browse_services(duration: Duration) -> Result<Vec<DiscoveredService>> {
    println!(
        "Browsing for _oscjson._tcp services for {} seconds...",
        duration.as_secs()
    );
    let mdns = ServiceDaemon::new().context("unable to start mDNS browser")?;
    let oscjson_rx = mdns
        .browse("_oscjson._tcp.local.")
        .context("failed to browse _oscjson._tcp.local.")?;
    let deadline = Instant::now() + duration;
    let mut services = HashMap::new();

    loop {
        match oscjson_rx.recv_deadline(deadline) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                let key = make_key("_oscjson._tcp", &info);
                services
                    .entry(key)
                    .or_insert_with(|| to_discovered("_oscjson._tcp", &info));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    mdns.shutdown().ok();
    Ok(services.into_values().collect())
}

fn make_key(service_type: &str, info: &ResolvedService) -> String {
    format!(
        "{}::{}::{}",
        service_type,
        info.get_fullname(),
        info.get_port()
    )
}

fn to_discovered(service_type: &'static str, info: &ResolvedService) -> DiscoveredService {
    let mut ipv4 = info
        .get_addresses_v4()
        .iter()
        .map(|ip| ip.to_string())
        .collect::<Vec<_>>();
    ipv4.sort();

    let mut txt = info
        .get_properties()
        .iter()
        .map(|prop| format!("{}={}", prop.key(), prop.val_str()))
        .collect::<Vec<_>>();
    txt.sort();

    DiscoveredService {
        service_type,
        instance: info.get_fullname().to_string(),
        hostname: info.get_hostname().to_string(),
        port: info.get_port(),
        ipv4,
        txt,
    }
}

fn print_browse(services: &[DiscoveredService], instance_filter: Option<&str>) {
    println!("=== OSCQuery mDNS Browse ===");
    println!("Looking for _oscjson._tcp");
    println!();

    let instance_filter = instance_filter.map(|n| n.to_ascii_lowercase());

    for svc in services {
        let mut marker = "[INFO]";
        if let Some(n) = &instance_filter {
            if svc.instance.to_ascii_lowercase().contains(n) {
                marker = "[FOUND]";
            }
        }
        println!("{} Service: {}", marker, svc.instance);
        println!("        Type:    {}", svc.service_type);
        println!("        Host:    {}", svc.hostname);
        println!("        Port:    {}", svc.port);
        if !svc.ipv4.is_empty() {
            println!("        IPv4:    {}", svc.ipv4.join(", "));
        }
        if !svc.txt.is_empty() {
            println!("        TXT:     {}", svc.txt.join(", "));
        }
        println!();
    }

    if services.is_empty() {
        println!("[FAIL] No OSCQuery services discovered.");
    } else {
        println!("[INFO] Found {} service(s) total.", services.len());
    }
}

fn run_auto(instance_filter: &str, duration: Duration) -> Result<()> {
    println!("=== OSCQuery Auto Mode ===");
    println!("Instance filter: {}", instance_filter);
    println!();

    let services = browse_services(duration)?;
    print_browse(&services, Some(instance_filter));

    let instance_filter_lc = instance_filter.to_ascii_lowercase();
    let candidates = services
        .iter()
        .filter(|svc| svc.service_type == "_oscjson._tcp")
        .filter(|svc| {
            svc.instance
                .to_ascii_lowercase()
                .contains(&instance_filter_lc)
        })
        .collect::<Vec<_>>();

    if candidates.is_empty() {
        bail!(
            "no matching _oscjson._tcp service found for instance filter '{}'",
            instance_filter
        );
    }

    let endpoints: Vec<String> = candidates
        .iter()
        .flat_map(|svc| {
            let hosts: Vec<&str> = if svc.ipv4.is_empty() {
                vec![svc.hostname.trim_end_matches('.')]
            } else {
                svc.ipv4.iter().map(String::as_str).collect()
            };
            hosts
                .into_iter()
                .map(move |host| format!("http://{}:{}", host, svc.port))
        })
        .collect();

    println!(
        "[INFO] {} endpoint(s) to query across {} candidate(s).",
        endpoints.len(),
        candidates.len()
    );
    println!();

    let mut success_count = 0usize;
    let mut failure_count = 0usize;

    for (idx, endpoint) in endpoints.iter().enumerate() {
        println!(
            "[INFO] Querying endpoint {}/{}: {}",
            idx + 1,
            endpoints.len(),
            endpoint
        );
        println!();

        match run_query(endpoint) {
            Ok(()) => success_count += 1,
            Err(err) => {
                failure_count += 1;
                println!("[WARN] Query failed for {}: {err}", endpoint);
                println!();
            }
        }
    }

    if success_count == 0 {
        bail!(
            "all {} endpoint(s) across {} candidate(s) failed to query",
            endpoints.len(),
            candidates.len()
        );
    }

    if failure_count > 0 {
        println!(
            "[WARN] Auto mode completed with partial failures: {}/{} endpoint(s) succeeded.",
            success_count,
            endpoints.len()
        );
    } else {
        println!(
            "[INFO] Auto mode completed: all {} endpoint(s) succeeded.",
            endpoints.len()
        );
    }
    Ok(())
}

fn run_query(endpoint: &str) -> Result<()> {
    let base = normalize_endpoint(endpoint);
    println!("=== OSCQuery Endpoint Probe: {} ===", base);
    println!();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to create HTTP client")?;

    println!("--- HOST_INFO ---");
    let host_info_url = format!("{}?HOST_INFO", base);
    println!("[INFO] Fetching {}", host_info_url);
    let host_info_bytes = get_bytes(&client, &host_info_url)?;
    println!("[PASS] Response received ({} bytes)", host_info_bytes.len());

    let parsed_host_info = serde_json::from_slice::<HostInfo>(&host_info_bytes).ok();
    if let Some(host_info) = &parsed_host_info {
        print_host_info(host_info);
    } else {
        println!(
            "[WARN] Cannot parse HOST_INFO JSON: {}",
            String::from_utf8_lossy(&host_info_bytes)
        );
    }
    println!();

    println!("--- OSC Address Tree ---");
    let root_url = format!("{base}/");
    println!("[INFO] Fetching {}", root_url);
    let root_bytes = get_bytes(&client, &root_url)?;
    println!("[PASS] Tree received ({} bytes)", root_bytes.len());
    if let Ok(root) = serde_json::from_slice::<Value>(&root_bytes) {
        let pretty =
            serde_json::to_string_pretty(&root).context("failed to pretty-print root tree JSON")?;
        println!("{pretty}");
    } else {
        println!("[WARN] Could not parse root tree JSON.");
    }
    Ok(())
}

fn normalize_endpoint(endpoint: &str) -> String {
    let host = endpoint.strip_prefix("http://").unwrap_or(endpoint);
    format!("http://{}", host.trim_end_matches('/'))
}

fn get_bytes(client: &Client, url: &str) -> Result<bytes::Bytes> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("HTTP request failed for {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {url}"))?;
    response
        .bytes()
        .with_context(|| format!("failed reading response body for {url}"))
}

fn print_host_info(info: &HostInfo) {
    println!(
        "       Name:      {}",
        info.name.as_deref().unwrap_or("<unknown>")
    );
    println!(
        "       OSC IP:    {}",
        info.osc_ip.as_deref().unwrap_or("<missing>")
    );
    println!(
        "       OSC Port:  {}",
        info.osc_port
            .map(|p| p.to_string())
            .as_deref()
            .unwrap_or("<missing>")
    );
    println!(
        "       Transport: {}",
        info.osc_transport.as_deref().unwrap_or("<missing>")
    );

    if let Some(exts) = &info.extensions {
        println!("       Extensions: {:?}", exts);
    }
}
