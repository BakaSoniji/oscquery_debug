use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::Value;

use crate::protocol::HostInfo;
use crate::report;

pub fn run_query(endpoint: &str) -> Result<()> {
    let base = normalize_endpoint(endpoint);
    println!("=== OSCQuery Endpoint Probe: {} ===", base);
    println!();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .context("failed to create HTTP client")?;

    println!("--- HOST_INFO ---");
    let host_info_url = format!("{}?HOST_INFO", base);
    report::info(format!("Fetching {}", host_info_url));
    let host_info_bytes = get_bytes(&client, &host_info_url)?;
    report::pass(format!(
        "Response received ({} bytes)",
        host_info_bytes.len()
    ));

    let parsed_host_info = serde_json::from_slice::<HostInfo>(&host_info_bytes).ok();
    if let Some(host_info) = &parsed_host_info {
        print_host_info(host_info);
    } else {
        report::warn(format!(
            "Cannot parse HOST_INFO JSON: {}",
            String::from_utf8_lossy(&host_info_bytes)
        ));
    }
    println!();

    println!("--- OSC Address Tree ---");
    let root_url = format!("{base}/");
    report::info(format!("Fetching {}", root_url));
    let root_bytes = get_bytes(&client, &root_url)?;
    report::pass(format!("Tree received ({} bytes)", root_bytes.len()));
    if let Ok(root) = serde_json::from_slice::<Value>(&root_bytes) {
        let pretty =
            serde_json::to_string_pretty(&root).context("failed to pretty-print root tree JSON")?;
        println!("{pretty}");
    } else {
        report::warn("Could not parse root tree JSON.");
    }
    Ok(())
}

fn normalize_endpoint(endpoint: &str) -> String {
    let host = endpoint.strip_prefix("http://").unwrap_or(endpoint);
    format!("http://{}", host.trim_end_matches('/'))
}

fn get_bytes(client: &Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("HTTP request failed for {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP error for {url}"))?;
    Ok(response
        .bytes()
        .with_context(|| format!("failed reading response body for {url}"))?
        .to_vec())
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
