#!/usr/bin/env python3
"""Canonical Harness test entrypoint for sfo-reuseport."""

from __future__ import annotations

import argparse
import platform
import subprocess
import sys
from pathlib import Path


TOKIO_URING_CHECK = [
    "cargo",
    "check",
    "--no-default-features",
    "--features",
    "runtime-tokio-uring",
    "--all-targets",
]


RUNTIME_FEATURES = {
    "runtime-tokio": ["--no-default-features", "--features", "runtime-tokio"],
    "runtime-async-std": ["--no-default-features", "--features", "runtime-async-std"],
    "runtime-tokio-uring": ["--no-default-features", "--features", "runtime-tokio-uring"],
}


def cargo_check_example(example: str, runtime: str) -> list[str]:
    return ["cargo", "check", *RUNTIME_FEATURES[runtime], "--example", example]


COMMANDS = {
    "sfo-reuseport": {
        "unit": [
            ["cargo", "test", "--lib"],
            ["cargo", "test", "--test", "unit"],
        ],
        "dv": [
            ["cargo", "check"],
            cargo_check_example("tcp_echo", "runtime-tokio"),
            cargo_check_example("tcp_echo", "runtime-async-std"),
            cargo_check_example("tcp_echo", "runtime-tokio-uring"),
            cargo_check_example("udp_server", "runtime-tokio"),
            cargo_check_example("udp_server", "runtime-async-std"),
            cargo_check_example("udp_server", "runtime-tokio-uring"),
            cargo_check_example("udp_serve_socket", "runtime-tokio"),
            cargo_check_example("udp_serve_socket", "runtime-async-std"),
            cargo_check_example("udp_serve_socket", "runtime-tokio-uring"),
            cargo_check_example("hyper_static", "runtime-tokio"),
            cargo_check_example("hyper_static", "runtime-async-std"),
            cargo_check_example("hyper_static", "runtime-tokio-uring"),
            ["cargo", "check", "--no-default-features", "--features", "runtime-async-std", "--lib"],
            TOKIO_URING_CHECK,
            ["cargo", "test", "--test", "dv"],
            ["python3", "./harness/scripts/test-hyper-static-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-hyper-static-example.py", "--runtime", "runtime-async-std"],
            ["python3", "./harness/scripts/test-hyper-static-example.py", "--runtime", "runtime-tokio-uring"],
            ["python3", "./harness/scripts/test-udp-server-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-udp-server-example.py", "--runtime", "runtime-async-std"],
            ["python3", "./harness/scripts/test-udp-server-example.py", "--runtime", "runtime-tokio-uring"],
            ["python3", "./harness/scripts/test-udp-serve-socket-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-udp-serve-socket-example.py", "--runtime", "runtime-async-std"],
            ["python3", "./harness/scripts/test-udp-serve-socket-example.py", "--runtime", "runtime-tokio-uring"],
        ],
        "integration": [["cargo", "test", "--test", "integration"]],
    }
}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("module", choices=sorted([*COMMANDS, "all"]))
    parser.add_argument("level", choices=("unit", "dv", "integration", "all"))
    parser.add_argument("--root", default=".")
    args = parser.parse_args()

    root = Path(args.root)
    modules = sorted(COMMANDS) if args.module == "all" else [args.module]
    levels = ("unit", "dv", "integration") if args.level == "all" else (args.level,)

    for module in modules:
        for level in levels:
            commands = COMMANDS[module][level]
            for command in commands:
                if "runtime-tokio-uring" in command and platform.system() != "Linux":
                    print(
                        f"test-run: {module} {level}: skip {' '.join(command)} "
                        "(runtime-tokio-uring is Linux-only)"
                    )
                    continue
                print(f"test-run: {module} {level}: {' '.join(command)}")
                code = subprocess.run(command, cwd=root).returncode
                if code != 0:
                    return code
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
