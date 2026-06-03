use std::env;
use std::io;
use std::net::SocketAddr;
use std::path::{Component, PathBuf};
use std::sync::Arc;

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use bytes::Bytes;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use http_body_util::Full;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper::body::Incoming;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper::header::{CONTENT_TYPE, HeaderValue};
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper::server::conn::http1;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper::service::service_fn;
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper::{Method, Request, Response, StatusCode};
#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
use hyper_util::rt::TokioIo;
use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, TcpServiceConfig, TcpServer};

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
type Body = Full<Bytes>;

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
#[tokio::main]
async fn main() -> Result<(), Error> {
    run_hyper().await
}

#[cfg(feature = "runtime-async-std")]
#[async_std::main]
async fn main() -> Result<(), Error> {
    run_plain().await
}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
async fn run_hyper() -> Result<(), Error> {
    let args = Args::parse()?;
    let root = Arc::new(canonical_root(args.root)?);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new())?;
    let config = TcpServiceConfig::new(args.addr);

    eprintln!("serving {} at http://{}", root.display(), args.addr);

    TcpServer::serve(&runtime, config, move |stream| {
        let root = Arc::clone(&root);
        async move {
            let service = service_fn(move |request| serve_request(Arc::clone(&root), request));
            http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await
                .map_err(|error| Error::Handler(format!("http connection failed: {error}")))
        }
    })?;
    std::future::pending::<Result<(), Error>>().await
}

#[cfg(feature = "runtime-async-std")]
async fn run_plain() -> Result<(), Error> {
    let args = Args::parse()?;
    let root = Arc::new(canonical_root(args.root)?);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new())?;
    let config = TcpServiceConfig::new(args.addr);

    eprintln!("serving {} at http://{}", root.display(), args.addr);

    TcpServer::serve(&runtime, config, move |stream| {
        let root = Arc::clone(&root);
        async move { serve_plain_connection(root, stream).await }
    })?;
    std::future::pending::<Result<(), Error>>().await
}

struct Args {
    addr: SocketAddr,
    root: PathBuf,
}

impl Args {
    fn parse() -> Result<Self, Error> {
        let mut addr = "127.0.0.1:8080"
            .parse()
            .map_err(|error| Error::InvalidConfig(format!("invalid default address: {error}")))?;
        let mut root = env::current_dir()?;
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
                "--root" => {
                    let value = args.next().ok_or_else(|| {
                        Error::InvalidConfig("--root requires a directory path".to_string())
                    })?;
                    root = PathBuf::from(value);
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

        Ok(Self { addr, root })
    }
}

fn print_usage() {
    println!("Usage: cargo run --example hyper_static -- [--root <path>] [--addr <addr>]");
}

fn canonical_root(root: PathBuf) -> Result<PathBuf, Error> {
    let root = root.canonicalize().map_err(|error| {
        Error::InvalidConfig(format!("static root `{}` is invalid: {error}", root.display()))
    })?;
    if !root.is_dir() {
        return Err(Error::InvalidConfig(format!(
            "static root `{}` is not a directory",
            root.display()
        )));
    }
    Ok(root)
}

#[cfg(feature = "runtime-async-std")]
async fn serve_plain_connection(
    root: Arc<PathBuf>,
    stream: sfo_reuseport::TcpStream,
) -> Result<(), Error> {
    let request = read_request(&stream).await?;
    if request.is_empty() {
        return Ok(());
    }
    let response = plain_response_for_request(&root, &request);
    write_response(&stream, response).await
}

#[cfg(feature = "runtime-async-std")]
async fn read_request(stream: &sfo_reuseport::TcpStream) -> Result<Vec<u8>, Error> {
    use async_std::io::ReadExt;

    let mut stream = stream;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while request.len() <= 8192 {
        let len = stream.read(&mut buffer).await?;
        if len == 0 {
            return Ok(Vec::new());
        }
        request.extend_from_slice(&buffer[..len]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
    }
    Err(Error::Handler("http request headers were not complete".to_string()))
}

#[cfg(feature = "runtime-async-std")]
fn plain_response_for_request(root: &PathBuf, request: &[u8]) -> Vec<u8> {
    let request = String::from_utf8_lossy(request);
    let Some(request_line) = request.lines().next() else {
        return plain_response(400, "Bad Request", "text/plain; charset=utf-8", b"bad request");
    };
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if method != "GET" && method != "HEAD" {
        return plain_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
        );
    }

    let headers_only = method == "HEAD";
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let path = match resolve_path(root, path) {
        Ok(path) => path,
        Err(ResolveError::Forbidden) => {
            return plain_response(403, "Forbidden", "text/plain; charset=utf-8", b"forbidden");
        }
        Err(ResolveError::NotFound) => {
            return plain_response(404, "Not Found", "text/plain; charset=utf-8", b"not found");
        }
    };

    match std::fs::read(&path) {
        Ok(bytes) => {
            let body = if headers_only { Vec::new() } else { bytes };
            plain_response(200, "OK", content_type_str(&path), &body)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            plain_response(404, "Not Found", "text/plain; charset=utf-8", b"not found")
        }
        Err(_) => plain_response(
            500,
            "Internal Server Error",
            "text/plain; charset=utf-8",
            b"internal server error",
        ),
    }
}

#[cfg(feature = "runtime-async-std")]
fn plain_response(status: u16, reason: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

#[cfg(feature = "runtime-async-std")]
async fn write_response(stream: &sfo_reuseport::TcpStream, response: Vec<u8>) -> Result<(), Error> {
    use async_std::io::WriteExt;

    let mut stream = stream;
    stream.write_all(&response).await?;
    Ok(())
}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
async fn serve_request(
    root: Arc<PathBuf>,
    request: Request<Incoming>,
) -> Result<Response<Body>, hyper::Error> {
    Ok(match *request.method() {
        Method::GET => response_for_path(&root, request.uri().path(), false).await,
        Method::HEAD => response_for_path(&root, request.uri().path(), true).await,
        _ => text_response(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
    })
}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
async fn response_for_path(root: &PathBuf, uri_path: &str, headers_only: bool) -> Response<Body> {
    let path = match resolve_path(root, uri_path) {
        Ok(path) => path,
        Err(ResolveError::Forbidden) => {
            return text_response(StatusCode::FORBIDDEN, "forbidden");
        }
        Err(ResolveError::NotFound) => {
            return text_response(StatusCode::NOT_FOUND, "not found");
        }
    };

    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let content_type = content_type(&path);
            let body = if headers_only { Vec::new() } else { bytes };
            let mut response = Response::new(Full::new(Bytes::from(body)));
            response.headers_mut().insert(CONTENT_TYPE, content_type);
            response
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            text_response(StatusCode::NOT_FOUND, "not found")
        }
        Err(_) => text_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
    }
}

fn resolve_path(root: &PathBuf, uri_path: &str) -> Result<PathBuf, ResolveError> {
    let mut candidate = root.clone();

    for raw_segment in uri_path.split('/') {
        if raw_segment.is_empty() {
            continue;
        }
        let segment = percent_decode(raw_segment)?;
        if segment == "." {
            continue;
        }
        if segment == ".."
            || segment.contains('\0')
            || segment.contains('/')
            || segment.contains('\\')
            || PathBuf::from(&segment)
                .components()
                .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        {
            return Err(ResolveError::Forbidden);
        }
        candidate.push(segment);
    }

    if candidate.is_dir() || uri_path.ends_with('/') {
        candidate.push("index.html");
    }

    let canonical = match candidate.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ResolveError::NotFound);
        }
        Err(_) => return Err(ResolveError::Forbidden),
    };

    if !canonical.starts_with(root) {
        return Err(ResolveError::Forbidden);
    }
    if !canonical.is_file() {
        return Err(ResolveError::NotFound);
    }

    Ok(canonical)
}

fn percent_decode(segment: &str) -> Result<String, ResolveError> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ResolveError::Forbidden);
            }
            let high = from_hex(bytes[index + 1])?;
            let low = from_hex(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| ResolveError::Forbidden)
}

fn from_hex(byte: u8) -> Result<u8, ResolveError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ResolveError::Forbidden),
    }
}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
fn text_response(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from_static(message.as_bytes())));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}

#[cfg(any(feature = "runtime-tokio", feature = "runtime-tokio-uring"))]
fn content_type(path: &std::path::Path) -> HeaderValue {
    HeaderValue::from_static(content_type_str(path))
}

fn content_type_str(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("txt") => "text/plain; charset=utf-8",
        Some("toml") => "text/plain; charset=utf-8",
        Some("md") => "text/plain; charset=utf-8",
        Some("yaml") => "text/plain; charset=utf-8",
        Some("bat") => "text/plain; charset=utf-8",
        Some("sh") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[derive(Debug)]
enum ResolveError {
    Forbidden,
    NotFound,
}
