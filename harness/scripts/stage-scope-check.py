#!/usr/bin/env python3
"""Validate that the current diff stays inside one Harness stage scope.

This checker is intentionally conservative and dependency-free. It is meant to
catch accidental cross-stage edits such as a proposal task also changing
design.md, testing.md, testplan.yaml, or acceptance artifacts.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


STAGES = {"proposal", "design", "testing", "implementation", "acceptance"}
MODULE_DOCS = {"proposal.md", "design.md", "testing.md", "testplan.yaml", "acceptance.md"}


def fail(message: str) -> None:
    print(f"stage-scope-check: {message}", file=sys.stderr)
    raise SystemExit(1)


def git(args: list[str], root: Path) -> str:
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=root,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=False,
        )
    except FileNotFoundError:
        fail("git executable not found")
    except subprocess.CalledProcessError as error:
        stderr = error.stderr.decode("utf-8", errors="replace").strip()
        fail(stderr or f"git {' '.join(args)} failed")
    return result.stdout.decode("utf-8", errors="replace")


def normalize(path: str) -> str:
    return path.replace("\\", "/").lstrip("./")


def parse_status_z(output: str) -> list[str]:
    changed: list[str] = []
    records = [record for record in output.split("\0") if record]
    index = 0
    while index < len(records):
        record = records[index]
        if len(record) < 4:
            fail(f"unexpected git status record: {record!r}")
        status = record[:2]
        path = record[3:]
        changed.append(normalize(path))
        index += 1
        if "R" in status or "C" in status:
            if index >= len(records):
                fail(f"rename/copy status missing old path for: {path}")
            changed.append(normalize(records[index]))
            index += 1
    return changed


def parse_diff_name_status_z(output: str) -> list[str]:
    changed: list[str] = []
    records = [record for record in output.split("\0") if record]
    index = 0
    while index < len(records):
        status = records[index]
        index += 1
        if not status:
            continue
        if status[0] in {"R", "C"}:
            if index + 1 >= len(records):
                fail(f"rename/copy diff status missing path data: {status}")
            old_path = records[index]
            new_path = records[index + 1]
            changed.extend([normalize(old_path), normalize(new_path)])
            index += 2
        else:
            if index >= len(records):
                fail(f"diff status missing path data: {status}")
            changed.append(normalize(records[index]))
            index += 1
    return changed


def changed_paths(root: Path, base: str | None, include_untracked: bool) -> list[str]:
    if base:
        output = git(["diff", "--name-status", "-z", f"{base}...HEAD"], root)
        return sorted(set(parse_diff_name_status_z(output)))

    args = ["status", "--porcelain=v1", "-z"]
    if include_untracked:
        args.append("--untracked-files=all")
    else:
        args.append("--untracked-files=no")
    output = git(args, root)
    return sorted(set(parse_status_z(output)))


def packet_parts(path: str) -> tuple[str, str, str] | None:
    parts = path.split("/")
    if len(parts) < 6:
        return None
    if parts[0] != "docs" or parts[1] != "versions" or parts[3] != "modules":
        return None
    version = parts[2]
    module = parts[4]
    relative = "/".join(parts[5:])
    return version, module, relative


def active_packet(path: str, version: str | None, module: str | None, submodule: str | None = None) -> tuple[str, str, str] | None:
    packet = packet_parts(path)
    if packet is None:
        return None
    packet_version, packet_module, relative = packet
    if version and packet_version != version:
        return None
    if module and packet_module != module:
        return None
    if submodule and relative != submodule and not relative.startswith(f"{submodule}/"):
        return None
    return packet_version, packet_module, relative


def is_module_boundary_sync(path: str, module: str | None) -> bool:
    if not path.startswith("docs/modules/") or not path.endswith(".md"):
        return False
    if module is None:
        return True
    return path == f"docs/modules/{module}.md"


def is_review_report(path: str) -> bool:
    if path.startswith("docs/reviews/") and path.endswith(".md"):
        return True
    parts = path.split("/")
    return (
        len(parts) >= 5
        and parts[0] == "docs"
        and parts[1] == "versions"
        and parts[3] == "reviews"
        and path.endswith(".md")
    )


def is_stage_doc_path(path: str) -> bool:
    packet = packet_parts(path)
    if packet is None:
        return False
    relative = packet[2]
    leaf = relative.rsplit("/", 1)[-1]
    return (
        leaf in MODULE_DOCS
        or relative.startswith("design/")
        or relative.startswith("testing/")
    )


def is_test_artifact(path: str) -> bool:
    parts = path.split("/")
    leaf = parts[-1].lower()
    return (
        "tests" in parts
        or "test" in parts
        or "__tests__" in parts
        or leaf.startswith("test_")
        or leaf.endswith("_test.py")
        or ".test." in leaf
        or ".spec." in leaf
        or leaf.endswith("_test.rs")
        or leaf.endswith("_tests.rs")
        or leaf.endswith("test.rs")
    )


def allowed_for_stage(path: str, stage: str, version: str | None, module: str | None, submodule: str | None = None) -> bool:
    packet = active_packet(path, version, module, submodule)
    relative = packet[2] if packet is not None else ""
    leaf = relative.rsplit("/", 1)[-1]

    if stage == "proposal":
        return packet is not None and leaf == "proposal.md"

    if stage == "design":
        if packet is not None and (leaf == "design.md" or relative.startswith("design/")):
            return True
        return is_module_boundary_sync(path, module)

    if stage == "testing":
        if packet is not None and (
            leaf == "testing.md"
            or leaf == "testplan.yaml"
            or relative.startswith("testing/")
        ):
            return True
        return is_test_artifact(path)

    if stage == "acceptance":
        return is_review_report(path)

    if stage == "implementation":
        if is_stage_doc_path(path) or is_review_report(path) or is_module_boundary_sync(path, module):
            return False
        if is_test_artifact(path):
            return False
        if path == "AGENTS.md" or path.startswith("harness/rules/") or path.startswith("harness/process_rules/"):
            return False
        return True

    fail(f"unknown stage: {stage}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--stage", required=True, choices=sorted(STAGES))
    parser.add_argument("--version")
    parser.add_argument("--module")
    parser.add_argument("--submodule")
    parser.add_argument("--base", help="compare committed changes from <base>...HEAD instead of the working tree")
    parser.add_argument("--ignore-untracked", action="store_true")
    args = parser.parse_args()

    root = Path(args.root)
    paths = changed_paths(root, args.base, include_untracked=not args.ignore_untracked)
    if not paths:
        print("stage-scope-check: no changed files")
        return 0

    violations = [
        path
        for path in paths
        if not allowed_for_stage(path, args.stage, args.version, args.module, args.submodule)
    ]
    if violations:
        print(f"stage-scope-check: {args.stage} stage scope violation", file=sys.stderr)
        for path in violations:
            print(f"  - {path}", file=sys.stderr)
        raise SystemExit(1)

    print(f"stage-scope-check: passed ({args.stage}, {len(paths)} changed path(s))")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
