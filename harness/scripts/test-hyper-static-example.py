#!/usr/bin/env python3
"""Run the hyper_static example and verify basic static-file behavior."""

from __future__ import annotations

import http.client
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def wait_for_server(port: int, process: subprocess.Popen[bytes]) -> None:
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"server exited early with code {process.returncode}")
        try:
            with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.1)
    raise RuntimeError("server did not start before timeout")


def request(port: int, path: str) -> tuple[int, bytes]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=5)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        return response.status, response.read()
    finally:
        connection.close()


def main() -> int:
    root = Path.cwd()
    port = free_port()

    with tempfile.TemporaryDirectory(prefix="sfo-reuseport-static-") as directory:
        static_root = Path(directory)
        (static_root / "hello.txt").write_text("hello from static example\n", encoding="utf-8")
        (static_root / "index.html").write_text("<h1>index</h1>\n", encoding="utf-8")

        process = subprocess.Popen(
            [
                "cargo",
                "run",
                "--quiet",
                "--example",
                "hyper_static",
                "--",
                "--root",
                str(static_root),
                "--addr",
                f"127.0.0.1:{port}",
            ],
            cwd=root,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )

        try:
            wait_for_server(port, process)

            status, body = request(port, "/hello.txt")
            if status != 200 or body != b"hello from static example\n":
                raise AssertionError(f"expected 200 hello.txt, got {status} {body!r}")

            status, body = request(port, "/")
            if status != 200 or b"<h1>index</h1>" not in body:
                raise AssertionError(f"expected 200 index.html, got {status} {body!r}")

            status, _ = request(port, "/missing.txt")
            if status != 404:
                raise AssertionError(f"expected 404 for missing file, got {status}")

            status, _ = request(port, "/../Cargo.toml")
            if status != 403:
                raise AssertionError(f"expected 403 for path traversal, got {status}")

            status, _ = request(port, "/%2e%2e/Cargo.toml")
            if status != 403:
                raise AssertionError(f"expected 403 for encoded path traversal, got {status}")
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

    print("hyper_static example smoke test passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
