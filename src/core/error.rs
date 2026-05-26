use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    InvalidConfig(String),
    UnsupportedPlatformOption(String),
    PermissionDenied(String),
    SocketInitCallback(String),
    Socket(io::Error),
    Runtime(String),
    UnknownListener,
    Handler(String),
}

impl Error {
    pub(crate) fn socket(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::PermissionDenied {
            Self::PermissionDenied(error.to_string())
        } else {
            Self::Socket(error)
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid config: {message}"),
            Self::UnsupportedPlatformOption(message) => {
                write!(f, "unsupported platform option: {message}")
            }
            Self::PermissionDenied(message) => write!(f, "permission denied: {message}"),
            Self::SocketInitCallback(message) => {
                write!(f, "socket init callback error: {message}")
            }
            Self::Socket(error) => write!(f, "socket error: {error}"),
            Self::Runtime(message) => write!(f, "runtime error: {message}"),
            Self::UnknownListener => write!(f, "unknown listener"),
            Self::Handler(message) => write!(f, "handler error: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Socket(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::socket(value)
    }
}
