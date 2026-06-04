use std::env;
use std::net::SocketAddr;

use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, UdpServiceConfig, UdpServer};

#[cfg(feature = "runtime-tokio")]
#[tokio::main]
async fn main() -> Result<(), Error> {
    run().await
}

#[cfg(feature = "runtime-async-std")]
#[async_std::main]
async fn main() -> Result<(), Error> {
    run().await
}

async fn run() -> Result<(), Error> {
    let args = Args::parse()?;
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(args.workers))?;
    let config = UdpServiceConfig::new(args.addr);

    eprintln!("udp echo server listening on {}", args.addr);

    UdpServer::serve(&runtime, config, |socket, meta, payload| async move {
        let Some(peer_addr) = meta.peer_addr else {
            return Ok(());
        };
        send_echo(socket, payload, peer_addr).await
    })?;

    std::future::pending::<Result<(), Error>>().await
}

async fn send_echo(
    socket: sfo_reuseport::UdpSocket,
    payload: Vec<u8>,
    peer_addr: SocketAddr,
) -> Result<(), Error> {
    socket.send_to(&payload, peer_addr).await?;
    Ok(())
}

struct Args {
    addr: SocketAddr,
    workers: usize,
}

impl Args {
    fn parse() -> Result<Self, Error> {
        let mut addr = "127.0.0.1:7001"
            .parse()
            .map_err(|error| Error::InvalidConfig(format!("invalid default address: {error}")))?;
        let mut workers = 4;
        let mut args = env::args().skip(1);

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--addr" => {
                    let value = args.next().ok_or_else(|| {
                        Error::InvalidConfig("--addr requires a socket address".to_string())
                    })?;
                    addr = value.parse().map_err(|error| {
                        Error::InvalidConfig(format!("invalid --addr value `{value}`: {error}"))
                    })?;
                }
                "--workers" => {
                    let value = args.next().ok_or_else(|| {
                        Error::InvalidConfig("--workers requires a positive integer".to_string())
                    })?;
                    workers = value.parse().map_err(|error| {
                        Error::InvalidConfig(format!("invalid --workers value `{value}`: {error}"))
                    })?;
                    if workers == 0 {
                        return Err(Error::InvalidConfig(
                            "--workers must be greater than zero".to_string(),
                        ));
                    }
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                _ => {
                    return Err(Error::InvalidConfig(format!(
                        "unknown argument `{arg}`; use --help"
                    )));
                }
            }
        }

        Ok(Self { addr, workers })
    }
}

fn print_usage() {
    println!("Usage: cargo run --example udp_server -- [--addr <addr>] [--workers <count>]");
}
