#!/usr/bin/env python3
"""Canonical Harness test entrypoint for sfo-reuseport."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


TOKIO_EXTERNAL_IO_URING_CHECK = [
    "cargo",
    "check",
    "--no-default-features",
    "--features",
    "runtime-tokio,tokio/io-uring",
    "--all-targets",
]


RUNTIME_FEATURES = {
    "runtime-tokio": ["--no-default-features", "--features", "runtime-tokio"],
    "runtime-async-std": ["--no-default-features", "--features", "runtime-async-std"],
}


def cargo_check_example(example: str, runtime: str) -> list[str]:
    return ["cargo", "check", *RUNTIME_FEATURES[runtime], "--example", example]


COMMANDS = {
    "sfo-reuseport": {
        "unit": [
            ["cargo", "test", "--lib"],
            ["cargo", "test", "--lib", "--features", "quinn"],
            ["cargo", "test", "--test", "unit"],
            ["cargo", "test", "--test", "unit", "--features", "quinn"],
        ],
        "dv": [
            ["cargo", "check"],
            cargo_check_example("tcp_echo", "runtime-tokio"),
            cargo_check_example("tcp_echo", "runtime-async-std"),
            cargo_check_example("udp_server", "runtime-tokio"),
            cargo_check_example("udp_server", "runtime-async-std"),
            cargo_check_example("udp_serve_socket", "runtime-tokio"),
            cargo_check_example("udp_serve_socket", "runtime-async-std"),
            cargo_check_example("hyper_static", "runtime-tokio"),
            cargo_check_example("hyper_static", "runtime-async-std"),
            ["cargo", "check", "--no-default-features", "--features", "runtime-async-std", "--lib"],
            ["cargo", "check", "--features", "quinn"],
            ["cargo", "check", "--no-default-features", "--features", "runtime-async-std,quinn", "--lib"],
            ["python3", "./harness/scripts/assert-no-cargo-feature.py", "runtime-tokio-uring"],
            TOKIO_EXTERNAL_IO_URING_CHECK,
            [
                "python3",
                "./harness/scripts/assert-no-cargo-package.py",
                "tokio-uring",
                "--features",
                "runtime-tokio,tokio/io-uring",
            ],
            ["cargo", "test", "--test", "dv"],
            ["python3", "./harness/scripts/test-hyper-static-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-hyper-static-example.py", "--runtime", "runtime-async-std"],
            ["python3", "./harness/scripts/test-udp-server-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-udp-server-example.py", "--runtime", "runtime-async-std"],
            ["python3", "./harness/scripts/test-udp-serve-socket-example.py", "--runtime", "runtime-tokio"],
            ["python3", "./harness/scripts/test-udp-serve-socket-example.py", "--runtime", "runtime-async-std"],
        ],
        "integration": [
            ["cargo", "test", "--test", "integration", "--", "--test-threads=1"],
            ["cargo", "test", "--test", "integration", "--features", "quinn", "--", "--test-threads=1"],
        ],
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
                print(f"test-run: {module} {level}: {' '.join(command)}")
                env = None
                if any("tokio/io-uring" in part for part in command):
                    env = os.environ.copy()
                    rustflags = env.get("RUSTFLAGS", "")
                    flags = rustflags.split()
                    if "--cfg" not in flags or "tokio_unstable" not in flags:
                        flags.extend(["--cfg", "tokio_unstable"])
                    env["RUSTFLAGS"] = " ".join(flags)
                code = subprocess.run(command, cwd=root, env=env).returncode
                if code != 0:
                    return code
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
