use std::env;
use std::net::SocketAddr;

use sfo_reuseport::{
    Error, ServerRuntime, ServerRuntimeConfig, UdpServiceConfig, UdpServer, UdpSocket,
};

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

#[cfg(feature = "runtime-tokio-uring")]
fn main() -> Result<(), Error> {
    tokio_uring::start(run())
}

async fn run() -> Result<(), Error> {
    let args = Args::parse()?;
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(args.workers))?;
    let config = UdpServiceConfig::new(args.addr);

    eprintln!(
        "udp serve_socket echo server listening on {} with {} worker(s)",
        args.addr, args.workers
    );

    let _server = UdpServer::serve_socket(&runtime, config, |socket, worker_id| async move {
        serve_socket_echo(socket, worker_id).await
    })?;

    std::future::pending::<Result<(), Error>>().await
}

async fn serve_socket_echo(socket: UdpSocket, worker_id: usize) -> Result<(), Error> {
    let local_addr = socket.local_addr()?;
    eprintln!("worker {worker_id} received UDP socket for {local_addr}");

    let mut buffer = vec![0_u8; 2048];
    loop {
        let (len, peer_addr) = socket.recv_from(&mut buffer).await?;
        socket.send_to(&buffer[..len], peer_addr).await?;
    }
}

struct Args {
    addr: SocketAddr,
    workers: usize,
}

impl Args {
    fn parse() -> Result<Self, Error> {
        let mut addr = "127.0.0.1:7002"
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
    println!(
        "Usage: cargo run --example udp_serve_socket -- [--addr <addr>] [--workers <count>]"
    );
}
