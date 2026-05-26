#!/usr/bin/env python3
"""Validate the generated Harness Engineering module packet shape.

This checker intentionally uses only the Python standard library.
Only proposal.md and design.md are mandatory implementation-admission inputs;
testing artifacts and testplan.yaml are validated when present.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


REQUIRED_FRONT_MATTER = ("module", "version", "status", "approved_by", "approved_at")
ALLOWED_LEVELS = {"unit", "dv", "integration"}
ALLOWED_MODES = {"enabled", "manual", "disabled"}


def fail(message: str) -> None:
    print(f"schema-check: {message}", file=sys.stderr)
    raise SystemExit(1)


def read_text(path: Path) -> str:
    if not path.exists():
        fail(f"missing required file: {path}")
    return path.read_text(encoding="utf-8")


def front_matter(text: str, path: Path) -> dict[str, str]:
    if not text.startswith("---\n"):
        fail(f"missing front matter: {path}")
    end = text.find("\n---", 4)
    if end == -1:
        fail(f"unterminated front matter: {path}")
    data: dict[str, str] = {}
    for line in text[4:end].splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        data[key.strip()] = value.strip()
    return data


def validate_doc(path: Path, module: str, version: str, submodule: str | None = None) -> None:
    data = front_matter(read_text(path), path)
    missing = [field for field in REQUIRED_FRONT_MATTER if field not in data]
    if missing:
        fail(f"{path} missing front matter fields: {', '.join(missing)}")
    if data["module"] != module:
        fail(f"{path} module mismatch: expected {module}, got {data['module']}")
    if data["version"] != version:
        fail(f"{path} version mismatch: expected {version}, got {data['version']}")
    if submodule and data.get("submodule") not in {None, "", submodule}:
        fail(f"{path} submodule mismatch: expected {submodule}, got {data['submodule']}")


def extract_level_blocks(text: str) -> dict[str, str]:
    match = re.search(r"(?m)^levels:\s*$", text)
    if not match:
        fail("testplan.yaml missing levels")
    levels_text = text[match.end() :]
    starts = list(re.finditer(r"(?m)^  ([A-Za-z0-9_-]+):\s*$", levels_text))
    blocks: dict[str, str] = {}
    for index, start in enumerate(starts):
        level = start.group(1)
        end = starts[index + 1].start() if index + 1 < len(starts) else len(levels_text)
        blocks[level] = levels_text[start.end() : end]
    return blocks


def validate_testplan(path: Path, module: str, version: str, submodule: str | None = None) -> None:
    text = read_text(path)
    for key, value in (("schema_version", "1"), ("version", version), ("module", module)):
        if not re.search(rf"(?m)^{re.escape(key)}:\s*{re.escape(value)}\s*$", text):
            fail(f"{path} missing or mismatched {key}: {value}")
    if submodule and re.search(r"(?m)^submodule:\s*\S+", text):
        if not re.search(rf"(?m)^submodule:\s*{re.escape(submodule)}\s*$", text):
            fail(f"{path} submodule mismatch: expected {submodule}")

    blocks = extract_level_blocks(text)
    unknown = set(blocks) - ALLOWED_LEVELS
    if unknown:
        fail(f"{path} has unknown test levels: {', '.join(sorted(unknown))}")

    step_ids: set[str] = set()
    for level in sorted(ALLOWED_LEVELS):
        if level not in blocks:
            fail(f"{path} missing test level: {level}")
        block = blocks[level]
        mode_match = re.search(r"(?m)^    mode:\s*([A-Za-z0-9_-]+)\s*$", block)
        if not mode_match:
            fail(f"{path} level {level} missing mode")
        mode = mode_match.group(1)
        if mode not in ALLOWED_MODES:
            fail(f"{path} level {level} has invalid mode: {mode}")

        ids = re.findall(r"(?m)^      - id:\s*([A-Za-z0-9_.-]+)\s*$", block)
        if mode == "enabled" and not ids:
            fail(f"{path} enabled level {level} has no steps")
        if mode in {"manual", "disabled"} and not re.search(r"(?mi)reason:\s*\S+", block):
            fail(f"{path} {mode} level {level} missing reason")
        for step_id in ids:
            if step_id in step_ids:
                fail(f"{path} duplicate step id: {step_id}")
            step_ids.add(step_id)
            step_pattern = rf"(?ms)^      - id:\s*{re.escape(step_id)}\s*$.*?^        name:\s*\S+.*?^        run:\s*\[.+\]\s*$"
            if not re.search(step_pattern, block):
                fail(f"{path} step {step_id} must define name and run")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--version", required=True)
    parser.add_argument("--module", required=True)
    parser.add_argument("--submodule")
    args = parser.parse_args()

    packet = Path(args.root) / "docs" / "versions" / args.version / "modules" / args.module
    if args.submodule:
        packet = packet / args.submodule
    for name in ("proposal.md", "design.md"):
        validate_doc(packet / name, args.module, args.version, args.submodule)
    optional_testing = packet / "testing.md"
    if optional_testing.exists():
        validate_doc(optional_testing, args.module, args.version, args.submodule)
    optional_testplan = packet / "testplan.yaml"
    if optional_testplan.exists():
        validate_testplan(optional_testplan, args.module, args.version, args.submodule)
    print("schema-check: passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
