use anyhow::Result;
use clap::Parser;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use upnpsim::cli::{Cli, Commands};
use upnpsim::protocol::{Request, Response};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start {
            listen,
            external_ip,
            interface,
            ttl_mode,
            socket_path,
        } => {
            tracing_subscriber::fmt::init();
            upnpsim::daemon::run(listen, external_ip, interface, ttl_mode, socket_path).await?;
        }
        Commands::Status { socket_path } => {
            let resp = send_command(&socket_path, &Request::Status).await?;
            print_response(&resp);
        }
        Commands::TimeShift {
            duration,
            socket_path,
        } => {
            let resp = send_command(&socket_path, &Request::TimeShift { duration }).await?;
            print_response(&resp);
        }
        Commands::SetTtlMode { mode, socket_path } => {
            let resp = send_command(&socket_path, &Request::SetTtlMode { mode }).await?;
            print_response(&resp);
        }
        Commands::ListMappings { socket_path } => {
            let resp = send_command(&socket_path, &Request::ListMappings).await?;
            print_response(&resp);
        }
        Commands::SetExternalIp { ip, socket_path } => {
            let resp = send_command(&socket_path, &Request::SetExternalIp { ip }).await?;
            print_response(&resp);
        }
        Commands::Shutdown { socket_path } => {
            let resp = send_command(&socket_path, &Request::Shutdown).await?;
            print_response(&resp);
        }
    }

    Ok(())
}

async fn send_command(socket_path: &str, request: &Request) -> Result<Response> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    let mut msg = serde_json::to_string(request)?;
    msg.push('\n');
    writer.write_all(msg.as_bytes()).await?;
    writer.shutdown().await?;

    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    reader.read_line(&mut line).await?;

    let resp: Response = serde_json::from_str(&line)?;
    Ok(resp)
}

fn print_response(resp: &Response) {
    if resp.success {
        if let Some(data) = &resp.data {
            println!("{}", serde_json::to_string_pretty(data).unwrap());
        } else {
            println!("OK");
        }
    } else {
        eprintln!("Error: {}", resp.error.as_deref().unwrap_or("unknown"));
        std::process::exit(1);
    }
}
