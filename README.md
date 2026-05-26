# sfo-reuseport

`sfo-reuseport` is a small Rust crate for building TCP and UDP services on
top of a worker runtime with reuse-port style socket distribution.

The crate exposes a compact API for:

- starting a configurable worker runtime
- serving TCP connections
- serving UDP datagrams
- routing QUIC-oriented UDP packets by connection-id shard
- selecting Tokio, async-std, or tokio-uring as the runtime backend
- applying socket options and custom socket initialization callbacks

## Status

This repository is managed with a Harness Engineering workflow. Versioned
proposal, design, testing, and acceptance artifacts live under
`docs/versions/`.

## Features

Exactly one runtime feature must be enabled:

- `runtime-tokio` is enabled by default.
- `runtime-async-std` selects the async-std backend.
- `runtime-tokio-uring` selects the tokio-uring backend and is Linux-only.

Example feature selection:

```sh
cargo check
cargo check --no-default-features --features runtime-async-std
cargo check --no-default-features --features runtime-tokio-uring
```

## Quick Start

Add the crate to a Rust project, then create a `ServerRuntime` and register a
service:

```rust
use std::net::SocketAddr;

use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, ServiceConfig, UdpServer};

async fn run() -> Result<(), Error> {
    let addr: SocketAddr = "127.0.0.1:7001"
        .parse()
        .map_err(|error| Error::InvalidConfig(format!("invalid bind address: {error}")))?;
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(4))?;
    let config = ServiceConfig::new(addr);

    UdpServer::serve(&runtime, config, |socket, meta, payload| async move {
        if let Some(peer_addr) = meta.peer_addr {
            socket.send_to(&payload, peer_addr).await?;
        }
        Ok(())
    })?;

    std::future::pending::<Result<(), Error>>().await
}
```

With the default Tokio runtime, the function can be launched from a Tokio main:

```rust
#[tokio::main]
async fn main() -> Result<(), sfo_reuseport::Error> {
    run().await
}
```

## Examples

Run a TCP echo server:

```sh
cargo run --example tcp_echo
```

Run a UDP echo server:

```sh
cargo run --example udp_server -- --addr 127.0.0.1:7001 --workers 4
```

Run the static HTTP example:

```sh
cargo run --example hyper_static -- --root . --addr 127.0.0.1:8080
```

## Public API

Core exports include:

- `ServerRuntime` and `ServerRuntimeConfig`
- `ServiceConfig`
- `TcpServer`
- `UdpServer`
- `QuicServer`
- `PacketMeta`
- `SocketOptions`
- `TransparentMode`
- `SocketInitCallback`
- runtime socket types `TcpStream` and `UdpSocket`
- crate error type `Error`

`ServiceConfig` starts with default socket options and can be customized with
`with_socket_options` or `with_socket_init_callback`.

## Socket Options

`SocketOptions::default()` enables `reuse_address` and disables IPv4
transparent binding. IPv4 transparent mode can be configured with:

- `TransparentMode::Disabled`
- `TransparentMode::Required`
- `TransparentMode::BestEffort`

Custom socket setup can be injected before common socket options are applied:

```rust
let config = ServiceConfig::new(addr).with_socket_init_callback(|socket| {
    socket.set_nonblocking(true)?;
    Ok(())
});
```

## Platform Notes

The crate uses platform-specific socket support where available and falls back
to userspace worker routing for runtimes that can support it. The
`runtime-tokio-uring` feature requires Linux.

QUIC routing is exposed through `QuicServer`. It routes UDP packets using the
first two bytes of the QUIC destination connection ID shard when the packet
shape provides one.

## Testing

Use the repository's canonical Harness entrypoint:

```sh
uv run --active python ./harness/scripts/test-run.py sfo-reuseport unit
uv run --active python ./harness/scripts/test-run.py sfo-reuseport dv
uv run --active python ./harness/scripts/test-run.py sfo-reuseport integration
uv run --active python ./harness/scripts/test-run.py sfo-reuseport all
```

The root shortcuts `test-run.sh` and `test-run.bat` prepare the local
environment and delegate to the same canonical test runner.
