use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};

use anyhow::{Context, Result};
use mdns_sd::{IfKind, ServiceDaemon, ServiceInfo};
use serde::{Deserialize, Serialize};

// ─── Service type constants ─────────────────────────────────────────────────

pub const SVC_OSCJSON_TCP: &str = "_oscjson._tcp";
pub const SVC_OSC_UDP: &str = "_osc._udp";
pub const MDNS_OSCJSON_TCP: &str = "_oscjson._tcp.local.";
pub const MDNS_OSC_UDP: &str = "_osc._udp.local.";

// ─── HostInfo ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct HostInfo {
    #[serde(rename = "NAME")]
    pub name: Option<String>,
    #[serde(rename = "OSC_IP")]
    pub osc_ip: Option<String>,
    #[serde(rename = "OSC_PORT")]
    pub osc_port: Option<u16>,
    #[serde(rename = "OSC_TRANSPORT")]
    pub osc_transport: Option<String>,
    #[serde(rename = "EXTENSIONS")]
    pub extensions: Option<BTreeMap<String, bool>>,
}

pub fn build_host_info(name: &str, ip: &str, osc_port: u16) -> HostInfo {
    HostInfo {
        name: Some(name.to_string()),
        osc_ip: Some(ip.to_string()),
        osc_port: Some(osc_port),
        osc_transport: Some("UDP".to_string()),
        extensions: Some(BTreeMap::from([("VALUE".to_string(), true)])),
    }
}

// ─── OSC address tree ───────────────────────────────────────────────────────

pub fn oscquery_tree_json() -> String {
    serde_json::json!({
        "FULL_PATH": "/",
        "CONTENTS": {
            "tracking": {
                "FULL_PATH": "/tracking",
                "CONTENTS": {
                    "vrsystem": {
                        "FULL_PATH": "/tracking/vrsystem",
                        "CONTENTS": {}
                    }
                }
            }
        }
    })
    .to_string()
}

// ─── mDNS registration ──────────────────────────────────────────────────────

pub fn register_mdns_service(
    mdns: &ServiceDaemon,
    service_type: &str,
    name: &str,
    hostname: &str,
    ip: Ipv4Addr,
    port: u16,
) -> Result<()> {
    let ip_str = ip.to_string();
    let mut info = ServiceInfo::new(
        service_type,
        name,
        hostname,
        ip_str.as_str(),
        port,
        None::<std::collections::HashMap<String, String>>,
    )
    .with_context(|| format!("failed to create {service_type} ServiceInfo"))?;
    info.set_interfaces(vec![IfKind::Addr(IpAddr::V4(ip))]);
    mdns.register(info)
        .with_context(|| format!("failed to register {service_type} service"))
}

// ─── DiscoveredService ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DiscoveredService {
    pub service_type: &'static str,
    pub instance: String,
    pub hostname: String,
    pub port: u16,
    pub ipv4: Vec<String>,
    pub txt: Vec<String>,
}