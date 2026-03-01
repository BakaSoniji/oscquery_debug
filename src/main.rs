mod browse;
mod listen;
mod osc;
mod protocol;
mod query;
mod report;

use std::time::Duration;

use anyhow::Result;
use clap::{Parser, Subcommand};

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

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Browse { seconds } => {
            let services = browse::browse_services(
                Duration::from_secs(seconds),
                |svc| browse::print_service(svc),
            )?;
            browse::print_summary(&services);
        }
        Commands::Query { endpoint } => {
            query::run_query(&endpoint)?;
        }
        Commands::Listen {
            osc_port,
            interface,
        } => {
            listen::run_listen(osc_port, interface)?;
        }
    }
    Ok(())
}
