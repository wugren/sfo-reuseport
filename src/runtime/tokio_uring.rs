#[cfg(not(target_os = "linux"))]
compile_error!("runtime-tokio-uring is only supported on Linux targets");

// `runtime-tokio-uring` keeps its feature and Linux cfg boundary, while network
// sockets use tokio's readiness-based TCP/UDP interfaces.
include!("tokio.rs");
