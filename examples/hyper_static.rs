#[cfg(feature = "runtime-tokio")]
use std::env;
#[cfg(feature = "runtime-tokio")]
use std::io;
#[cfg(feature = "runtime-tokio")]
use std::net::SocketAddr;
#[cfg(feature = "runtime-tokio")]
use std::path::{Component, PathBuf};
#[cfg(feature = "runtime-tokio")]
use std::sync::Arc;

#[cfg(feature = "runtime-tokio")]
use bytes::Bytes;
#[cfg(feature = "runtime-tokio")]
use http_body_util::Full;
#[cfg(feature = "runtime-tokio")]
use hyper::body::Incoming;
#[cfg(feature = "runtime-tokio")]
use hyper::header::{CONTENT_TYPE, HeaderValue};
#[cfg(feature = "runtime-tokio")]
use hyper::server::conn::http1;
#[cfg(feature = "runtime-tokio")]
use hyper::service::service_fn;
#[cfg(feature = "runtime-tokio")]
use hyper::{Method, Request, Response, StatusCode};
#[cfg(feature = "runtime-tokio")]
use hyper_util::rt::TokioIo;
#[cfg(feature = "runtime-tokio")]
use sfo_reuseport::{Error, ServerRuntime, ServerRuntimeConfig, ServiceConfig, TcpServer};

#[cfg(feature = "runtime-tokio")]
type Body = Full<Bytes>;

#[cfg(feature = "runtime-tokio")]
#[tokio::main]
async fn main() -> Result<(), Error> {
    let args = Args::parse()?;
    let root = Arc::new(canonical_root(args.root)?);
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new())?;
    let config = ServiceConfig::new(args.addr);

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

#[cfg(not(feature = "runtime-tokio"))]
fn main() {}

#[cfg(feature = "runtime-tokio")]
struct Args {
    addr: SocketAddr,
    root: PathBuf,
}

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
fn print_usage() {
    println!("Usage: cargo run --example hyper_static -- [--root <path>] [--addr <addr>]");
}

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
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

#[cfg(feature = "runtime-tokio")]
fn from_hex(byte: u8) -> Result<u8, ResolveError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ResolveError::Forbidden),
    }
}

#[cfg(feature = "runtime-tokio")]
fn text_response(status: StatusCode, message: &'static str) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::from_static(message.as_bytes())));
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}

#[cfg(feature = "runtime-tokio")]
fn content_type(path: &std::path::Path) -> HeaderValue {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("html" | "htm") => HeaderValue::from_static("text/html; charset=utf-8"),
        Some("css") => HeaderValue::from_static("text/css; charset=utf-8"),
        Some("js") => HeaderValue::from_static("application/javascript"),
        Some("json") => HeaderValue::from_static("application/json"),
        Some("png") => HeaderValue::from_static("image/png"),
        Some("jpg" | "jpeg") => HeaderValue::from_static("image/jpeg"),
        Some("gif") => HeaderValue::from_static("image/gif"),
        Some("svg") => HeaderValue::from_static("image/svg+xml"),
        Some("txt") => HeaderValue::from_static("text/plain; charset=utf-8"),
        Some("toml") => HeaderValue::from_static("text/plain; charset=utf-8"),
        Some("md") => HeaderValue::from_static("text/plain; charset=utf-8"),
        Some("yaml") => HeaderValue::from_static("text/plain; charset=utf-8"),
        Some("bat") => HeaderValue::from_static("text/plain; charset=utf-8"),
        Some("sh") => HeaderValue::from_static("text/plain; charset=utf-8"),
        _ => HeaderValue::from_static("application/octet-stream"),
    }
}

#[cfg(feature = "runtime-tokio")]
#[derive(Debug)]
enum ResolveError {
    Forbidden,
    NotFound,
}
