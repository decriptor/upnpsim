use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "upnpsim", about = "UPnP IGD Router Simulator")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the UPnP simulator daemon
    Start {
        /// HTTP listen address
        #[arg(long, default_value = "0.0.0.0:5000")]
        listen: String,

        /// External (WAN) IP address to report
        #[arg(long, default_value = "203.0.113.1")]
        external_ip: String,

        /// Network interface address for SSDP multicast
        #[arg(long, default_value = "0.0.0.0")]
        interface: String,

        /// TTL mode: respect or disrespect
        #[arg(long, default_value = "respect")]
        ttl_mode: String,

        /// Unix socket path for control commands
        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// Query daemon status
    Status {
        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// Shift the virtual clock forward
    TimeShift {
        /// Duration to shift (e.g. "30s", "1h", "2h30m")
        duration: String,

        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// Set TTL mode (respect / disrespect)
    SetTtlMode {
        /// Mode: respect or disrespect
        mode: String,

        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// List all port mappings
    ListMappings {
        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// Set the external IP address
    SetExternalIp {
        /// New external IP
        ip: String,

        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },

    /// Shut down the daemon
    Shutdown {
        #[arg(long, default_value = "/tmp/upnpsim.sock")]
        socket_path: String,
    },
}
