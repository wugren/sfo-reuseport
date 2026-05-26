#[cfg(not(feature = "runtime-tokio-uring"))]
use std::io::ErrorKind;
#[cfg(not(feature = "runtime-tokio-uring"))]
use std::net::SocketAddr;

#[cfg(not(feature = "runtime-tokio-uring"))]
use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer};

#[cfg(feature = "runtime-tokio-uring")]
fn main() {}

#[cfg(not(feature = "runtime-tokio-uring"))]
#[tokio::main]
async fn main() -> Result<(), Error> {
    let bind_addr: SocketAddr = "127.0.0.1:7000"
        .parse()
        .map_err(|error| Error::InvalidConfig(format!("invalid bind address: {error}")))?;
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(4))?;
    let config = ServiceConfig::new(bind_addr);

    TcpServer::serve(&runtime, config, |stream| async move {
        echo_connection(stream).await
    })
    .await
}

#[cfg(not(feature = "runtime-tokio-uring"))]
async fn echo_connection(stream: sfo_reuseport::TcpStream) -> Result<(), Error> {
    let mut buffer = [0_u8; 4096];

    loop {
        stream.readable().await?;
        match stream.try_read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(len) => write_all(&stream, &buffer[..len]).await?,
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(not(feature = "runtime-tokio-uring"))]
async fn write_all(stream: &sfo_reuseport::TcpStream, mut bytes: &[u8]) -> Result<(), Error> {
    while !bytes.is_empty() {
        stream.writable().await?;
        match stream.try_write(bytes) {
            Ok(0) => {
                return Err(Error::Runtime(
                    "tcp stream closed before all bytes were written".to_string(),
                ));
            }
            Ok(len) => bytes = &bytes[len..],
            Err(error) if error.kind() == ErrorKind::WouldBlock => continue,
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}
