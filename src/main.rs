mod browse;
mod listen;
mod osc;
mod protocol;
mod query;
mod report;

use std::io::{Write, stdin, stdout};
use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "oscquery_debug", version)]
#[command(about = "SlimeVR/VRChat OSCQuery debugger")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Browse mDNS for OSCQuery services.
    Browse {
        /// Browse time in seconds.
        #[arg(default_value_t = 15)]
        seconds: u64,
    },
    /// Query a specific OSCQuery endpoint (host:port or URL).
    Query { endpoint: String },
    /// Stub SlimeVR's OSCQuery role: advertise mDNS, serve OSCQuery HTTP, listen for OSC data.
    Listen {
        /// OSC UDP port to listen on (SlimeVR default: 9001).
        #[arg(long, default_value_t = 9001)]
        osc_port: u16,
        /// Network interface IP to advertise on. If omitted, auto-selects or prompts.
        #[arg(long)]
        interface: Option<String>,
    },
}

/// Interactive menu shown when no subcommand is given (e.g. double-clicking on Windows).
fn interactive_menu() -> Result<()> {
    println!("SlimeVR/VRChat OSCQuery debugger\n");
    println!("  1) Browse   - discover mDNS OSCQuery services");
    println!("  2) Query    - query a specific OSCQuery endpoint");
    println!("  3) Listen   - stub SlimeVR's OSCQuery role");
    println!("  4) Exit\n");

    loop {
        print!("Select [1-4]: ");
        stdout().flush()?;
        let mut input = String::new();
        stdin().read_line(&mut input)?;
        match input.trim() {
            "1" => {
                print!("Browse time in seconds [15]: ");
                stdout().flush()?;
                let mut secs = String::new();
                stdin().read_line(&mut secs)?;
                let secs: u64 = secs.trim().parse().unwrap_or(15);
                let services = browse::browse_services(
                    Duration::from_secs(secs),
                    |svc| browse::print_service(svc),
                )?;
                browse::print_summary(&services);
                return Ok(());
            }
            "2" => {
                print!("Endpoint (host:port or URL): ");
                stdout().flush()?;
                let mut ep = String::new();
                stdin().read_line(&mut ep)?;
                let ep = ep.trim();
                if ep.is_empty() {
                    println!("No endpoint provided.");
                    continue;
                }
                query::run_query(ep)?;
                return Ok(());
            }
            "3" => {
                print!("OSC port [9001]: ");
                stdout().flush()?;
                let mut port = String::new();
                stdin().read_line(&mut port)?;
                let port: u16 = port.trim().parse().unwrap_or(9001);
                listen::run_listen(port, None)?;
                return Ok(());
            }
            "4" | "q" | "quit" | "exit" => return Ok(()),
            _ => println!("Invalid choice, try again."),
        }
    }
}

/// When running interactively (no args), pause before exiting so the window stays open.
fn pause_before_exit() {
    print!("\nPress Enter to exit...");
    let _ = stdout().flush();
    let _ = stdin().read_line(&mut String::new());
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let interactive = cli.command.is_none();

    let result = match cli.command {
        None => interactive_menu(),
        Some(Commands::Browse { seconds }) => {
            let services = browse::browse_services(
                Duration::from_secs(seconds),
                |svc| browse::print_service(svc),
            )?;
            browse::print_summary(&services);
            Ok(())
        }
        Some(Commands::Query { endpoint }) => query::run_query(&endpoint),
        Some(Commands::Listen {
            osc_port,
            interface,
        }) => listen::run_listen(osc_port, interface),
    };

    if interactive {
        if let Err(ref e) = result {
            eprintln!("\nError: {e:?}");
        }
        pause_before_exit();
    }

    result
}
