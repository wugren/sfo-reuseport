#!/usr/bin/env python3
"""Run the udp_serve_socket example and verify UDP echo behavior."""

from __future__ import annotations

import argparse
import errno
import socket
import subprocess
import sys
import time
from pathlib import Path


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_echo(port: int, process: subprocess.Popen[bytes]) -> None:
    deadline = time.monotonic() + 30
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client:
        client.bind(("127.0.0.1", 0))
        disable_udp_connreset(client)
        client.settimeout(0.25)
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(f"server exited early with code {process.returncode}")

            client.sendto(b"serve-socket", ("127.0.0.1", port))
            try:
                data, peer = client.recvfrom(64)
            except TimeoutError:
                continue
            except socket.timeout:
                continue
            except OSError as error:
                if is_udp_probe_reset(error) and process.poll() is None:
                    continue
                raise

            if peer[0] == "127.0.0.1" and peer[1] == port and data == b"serve-socket":
                return
            raise AssertionError(f"expected echo from server, got {data!r} from {peer!r}")

    raise RuntimeError("server did not echo UDP packet before timeout")


def disable_udp_connreset(sock: socket.socket) -> None:
    udp_connreset = getattr(socket, "SIO_UDP_CONNRESET", None)
    if udp_connreset is None:
        return
    try:
        sock.ioctl(udp_connreset, False)
    except OSError:
        pass


def is_udp_probe_reset(error: OSError) -> bool:
    winerror = getattr(error, "winerror", None)
    return (
        isinstance(error, ConnectionResetError)
        or error.errno == errno.ECONNRESET
        or winerror == 10054
    )


def feature_args(runtime: str) -> list[str]:
    return ["--no-default-features", "--features", runtime]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--runtime",
        choices=("runtime-tokio", "runtime-async-std"),
        default="runtime-tokio",
    )
    args = parser.parse_args()

    root = Path.cwd()
    port = free_port()

    process = subprocess.Popen(
        [
            "cargo",
            "run",
            "--quiet",
            *feature_args(args.runtime),
            "--example",
            "udp_serve_socket",
            "--",
            "--addr",
            f"127.0.0.1:{port}",
            "--workers",
            "1",
        ],
        cwd=root,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    try:
        wait_for_echo(port, process)
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)

        stderr = process.stderr.read().decode("utf-8", errors="replace")
        if process.returncode not in (0, -15, 143):
            print(stderr, file=sys.stderr)

    print(f"udp_serve_socket example smoke test passed for {args.runtime}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
