use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};

use crate::protocol::{
    DiscoveredService, MDNS_OSC_UDP, MDNS_OSCJSON_TCP, SVC_OSC_UDP, SVC_OSCJSON_TCP,
};
use crate::report;

pub fn browse_services(
    duration: Duration,
    mut on_discover: impl FnMut(&DiscoveredService),
) -> Result<Vec<DiscoveredService>> {
    report::info(format!(
        "Browsing for {} and {} services for {} seconds...",
        SVC_OSCJSON_TCP,
        SVC_OSC_UDP,
        duration.as_secs()
    ));
    let mdns = ServiceDaemon::new().context("unable to start mDNS browser")?;
    let oscjson_rx = mdns
        .browse(MDNS_OSCJSON_TCP)
        .context("failed to browse _oscjson._tcp.local.")?;
    let osc_udp_rx = mdns
        .browse(MDNS_OSC_UDP)
        .context("failed to browse _osc._udp.local.")?;
    let deadline = Instant::now() + duration;
    let mut services = HashMap::new();

    // Merge both receivers into a single channel using forwarding threads
    let (merged_tx, merged_rx) = std::sync::mpsc::channel();
    let merged_tx_udp = merged_tx.clone();
    let oscjson_forwarder = std::thread::spawn(move || {
        while let Ok(event) = oscjson_rx.recv() {
            if merged_tx.send((SVC_OSCJSON_TCP, event)).is_err() {
                break;
            }
        }
    });
    let osc_udp_forwarder = std::thread::spawn(move || {
        while let Ok(event) = osc_udp_rx.recv() {
            if merged_tx_udp.send((SVC_OSC_UDP, event)).is_err() {
                break;
            }
        }
    });

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match merged_rx.recv_timeout(remaining) {
            Ok((svc_type, ServiceEvent::ServiceResolved(info))) => {
                let key = make_key(svc_type, &info);
                if !services.contains_key(&key) {
                    let discovered = to_discovered(svc_type, &info);
                    on_discover(&discovered);
                    services.insert(key, discovered);
                }
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    mdns.shutdown().ok();
    drop(merged_rx);
    oscjson_forwarder.join().ok();
    osc_udp_forwarder.join().ok();
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

pub fn print_service(svc: &DiscoveredService) {
    report::info(format!("Service: {}", svc.instance));
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

pub fn print_summary(services: &[DiscoveredService]) {
    if services.is_empty() {
        report::fail("No services discovered.");
    } else {
        let oscjson_count = services
            .iter()
            .filter(|s| s.service_type == SVC_OSCJSON_TCP)
            .count();
        let osc_udp_count = services
            .iter()
            .filter(|s| s.service_type == SVC_OSC_UDP)
            .count();
        report::info(format!(
            "Found {} service(s) total ({} {}, {} {}).",
            services.len(),
            oscjson_count,
            SVC_OSCJSON_TCP,
            osc_udp_count,
            SVC_OSC_UDP,
        ));
    }
}
