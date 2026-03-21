use std::collections::HashMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mdns_sd::{IfKind, ResolvedService, ServiceDaemon, ServiceEvent};

use crate::protocol::{
    DiscoveredService, MDNS_OSC_UDP, MDNS_OSCJSON_TCP, SVC_OSC_UDP, SVC_OSCJSON_TCP,
};
use crate::report;

/// Discover unique non-loopback interface names (deduplicated across IPv4/IPv6).
fn interface_names() -> Vec<String> {
    let Ok(addrs) = if_addrs::get_if_addrs() else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    for iface in addrs {
        if iface.is_loopback() {
            continue;
        }
        if !seen.contains(&iface.name) {
            seen.push(iface.name);
        }
    }
    seen
}

pub fn browse_services(
    duration: Duration,
    mut on_discover: impl FnMut(&DiscoveredService),
) -> Result<Vec<DiscoveredService>> {
    let ifaces = interface_names();
    if ifaces.is_empty() {
        anyhow::bail!("no non-loopback network interfaces found");
    }

    report::info(format!(
        "Browsing for {} and {} services for {} seconds on {} interface(s): {}",
        SVC_OSCJSON_TCP,
        SVC_OSC_UDP,
        duration.as_secs(),
        ifaces.len(),
        ifaces.join(", "),
    ));

    let (tx, rx) = mpsc::channel::<(String, &'static str, ServiceEvent)>();
    let mut daemons = Vec::new();
    let mut forwarders = Vec::new();

    for iface in &ifaces {
        let mdns = ServiceDaemon::new()
            .with_context(|| format!("unable to start mDNS browser for {iface}"))?;
        mdns.disable_interface(IfKind::All)
            .with_context(|| format!("failed to disable all interfaces for {iface}"))?;
        mdns.enable_interface(IfKind::Name(iface.clone()))
            .with_context(|| format!("failed to enable interface {iface}"))?;

        for &(mdns_type, svc_label) in &[
            (MDNS_OSCJSON_TCP, SVC_OSCJSON_TCP),
            (MDNS_OSC_UDP, SVC_OSC_UDP),
        ] {
            let svc_rx = mdns
                .browse(mdns_type)
                .with_context(|| format!("failed to browse {mdns_type} on {iface}"))?;
            let tx = tx.clone();
            let iface = iface.clone();
            forwarders.push(std::thread::spawn(move || {
                while let Ok(event) = svc_rx.recv() {
                    if tx.send((iface.clone(), svc_label, event)).is_err() {
                        break;
                    }
                }
            }));
        }

        daemons.push(mdns);
    }
    drop(tx);

    let deadline = Instant::now() + duration;
    let mut services = HashMap::new();

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match rx.recv_timeout(remaining) {
            Ok((iface, svc_type, ServiceEvent::ServiceResolved(info))) => {
                let key = make_key(&iface, svc_type, &info);
                if !services.contains_key(&key) {
                    let discovered = to_discovered(&iface, svc_type, &info);
                    on_discover(&discovered);
                    services.insert(key, discovered);
                }
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    for mdns in daemons {
        mdns.shutdown().ok();
    }
    drop(rx);
    for f in forwarders {
        f.join().ok();
    }
    Ok(services.into_values().collect())
}

fn make_key(iface: &str, service_type: &str, info: &ResolvedService) -> String {
    format!(
        "{}::{}::{}::{}",
        iface,
        service_type,
        info.get_fullname(),
        info.get_port()
    )
}

fn to_discovered(
    iface: &str,
    service_type: &'static str,
    info: &ResolvedService,
) -> DiscoveredService {
    let mut addresses = info
        .get_addresses()
        .iter()
        .map(|ip| ip.to_ip_addr().to_string())
        .collect::<Vec<_>>();
    addresses.sort();

    let mut txt = info
        .get_properties()
        .iter()
        .map(|prop| format!("{}={}", prop.key(), prop.val_str()))
        .collect::<Vec<_>>();
    txt.sort();

    DiscoveredService {
        interface: iface.to_string(),
        service_type,
        instance: info.get_fullname().to_string(),
        hostname: info.get_hostname().to_string(),
        port: info.get_port(),
        addresses,
        txt,
    }
}

pub fn print_service(svc: &DiscoveredService) {
    report::info(format!("Service: {}", svc.instance));
    println!("        Interface: {}", svc.interface);
    println!("        Type:    {}", svc.service_type);
    println!("        Host:    {}", svc.hostname);
    println!("        Port:    {}", svc.port);
    if !svc.addresses.is_empty() {
        println!("        Addresses:    {}", svc.addresses.join(", "));
    }
    if !svc.txt.is_empty() {
        println!("        TXT:     {}", svc.txt.join(", "));
    }
    println!();
}

pub fn print_summary(services: &[DiscoveredService]) {
    if services.is_empty() {
        report::fail("No services discovered.");
        return;
    }

    // Group by interface
    let mut by_iface: Vec<(&str, Vec<&DiscoveredService>)> = Vec::new();
    for svc in services {
        if let Some(entry) = by_iface.iter_mut().find(|(name, _)| *name == svc.interface) {
            entry.1.push(svc);
        } else {
            by_iface.push((&svc.interface, vec![svc]));
        }
    }

    println!();
    for (iface, svcs) in &by_iface {
        println!("── {} ──", iface);
        for svc in svcs {
            let addrs = if svc.addresses.is_empty() {
                String::new()
            } else {
                format!("  {}", svc.addresses.join(", "))
            };
            println!(
                "  {}  {}  :{}{}",
                svc.instance, svc.service_type, svc.port, addrs
            );
        }
        println!();
    }

    // Deduplicated count: unique (instance, service_type) pairs across interfaces
    let mut unique = services
        .iter()
        .map(|s| (&s.instance, s.service_type))
        .collect::<Vec<_>>();
    unique.sort();
    unique.dedup();
    let unique_oscjson = unique
        .iter()
        .filter(|(_, t)| *t == SVC_OSCJSON_TCP)
        .count();
    let unique_osc_udp = unique
        .iter()
        .filter(|(_, t)| *t == SVC_OSC_UDP)
        .count();
    report::info(format!(
        "Found {} unique service(s) ({} {}, {} {}) across {} interface(s).",
        unique.len(),
        unique_oscjson,
        SVC_OSCJSON_TCP,
        unique_osc_udp,
        SVC_OSC_UDP,
        by_iface.len(),
    ));
}
