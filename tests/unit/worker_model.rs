use sfo_reuseport::{ServerRuntimeConfig, WorkerCount};

#[test]
fn server_runtime_config_can_set_explicit_worker_count() {
    let config = ServerRuntimeConfig::new().with_workers(2);
    assert_eq!(config.workers, WorkerCount::Fixed(2));
}
