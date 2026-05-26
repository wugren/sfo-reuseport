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

COMMANDS = {
    "sfo-reuseport": {
        "unit": [
            ["cargo", "test", "--lib"],
            ["cargo", "test", "--test", "unit"],
        ],
        "dv": [
            ["cargo", "check"],
            ["cargo", "check", "--example", "hyper_static"],
            ["cargo", "check", "--no-default-features", "--features", "runtime-async-std", "--lib"],
            TOKIO_URING_CHECK,
            ["cargo", "test", "--test", "dv"],
            ["python3", "./harness/scripts/test-hyper-static-example.py"],
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
                if command == TOKIO_URING_CHECK and platform.system() != "Linux":
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
