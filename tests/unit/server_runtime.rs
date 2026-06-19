use std::cell::RefCell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use sfo_reuseport::{
    QuicServer, ServerRuntime, ServerRuntimeConfig, TcpServer, TcpServiceConfig, UdpServer,
    UdpServiceConfig, WorkerCount,
};

#[test]
fn server_runtime_config_can_set_worker_count() {
    let config = ServerRuntimeConfig::new().with_workers(2);
    assert_eq!(config.workers, WorkerCount::Fixed(2));
}

#[test]
fn service_config_records_bind_addr_without_worker_count() {
    let addr = "127.0.0.1:0".parse().unwrap();
    let config = UdpServiceConfig::new(addr);
    assert_eq!(config.bind_addr, addr);
}

#[test]
fn server_runtime_spawn_task_accepts_factory_api() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let (sender, receiver) = mpsc::channel();

    let _task = runtime
        .spawn_task(move || -> Pin<Box<dyn Future<Output = ()> + 'static>> {
            Box::pin(async move {
                sender.send("completed").unwrap();
            })
        })
        .unwrap();

    assert_eq!(
        receiver.recv_timeout(Duration::from_secs(5)).unwrap(),
        "completed"
    );
}

#[test]
fn server_runtime_spawn_runs_task_on_worker_runtime() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let caller_thread = thread::current().id();
    let (sender, receiver) = mpsc::channel();

    let _task = runtime
        .spawn_task(move || {
            Box::pin(async move {
                sender.send(thread::current().id()).unwrap();
            })
        })
        .unwrap();

    let worker_thread = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(worker_thread, caller_thread);
}

#[test]
fn server_runtime_spawn_task_from_worker_thread_spawns_locally() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let runtime_from_worker = runtime.clone();
    let caller_thread = thread::current().id();
    let (sender, receiver) = mpsc::channel();

    let _outer_task = runtime
        .spawn_task(move || {
            Box::pin(async move {
                let outer_worker_thread = thread::current().id();
                let sender = sender.clone();
                let _inner_task = runtime_from_worker
                    .spawn_task(move || {
                        Box::pin(async move {
                            sender
                                .send((outer_worker_thread, thread::current().id()))
                                .unwrap();
                        })
                    })
                    .unwrap();
            })
        })
        .unwrap();

    let (outer_worker_thread, inner_worker_thread) =
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_ne!(outer_worker_thread, caller_thread);
    assert_eq!(inner_worker_thread, outer_worker_thread);
}

#[test]
fn server_runtime_spawn_task_future_can_use_worker_local_non_send_state() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();
    let (sender, receiver) = mpsc::channel();

    let _task = runtime
        .spawn_task(move || -> Pin<Box<dyn Future<Output = ()> + 'static>> {
            Box::pin(async move {
                let value = Rc::new(RefCell::new(0usize));
                *value.borrow_mut() += 1;
                sender.send(*value.borrow()).unwrap();
            })
        })
        .unwrap();

    assert_eq!(receiver.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
}

#[test]
fn servers_return_handles_when_attached_to_server_runtime_through_serve() {
    let runtime = ServerRuntime::start(ServerRuntimeConfig::new().with_workers(1)).unwrap();

    let tcp = TcpServer::serve(
        &runtime,
        TcpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_stream| async { Ok(()) },
    )
    .unwrap();
    let udp = UdpServer::serve(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_socket, _meta, _payload| async { Ok(()) },
    )
    .unwrap();
    let quic = QuicServer::serve(
        &runtime,
        UdpServiceConfig::new("127.0.0.1:0".parse().unwrap()),
        |_socket, _meta, _payload| async { Ok(()) },
    )
    .unwrap();

    tcp.close().unwrap();
    udp.close().unwrap();
    quic.close().unwrap();
}
