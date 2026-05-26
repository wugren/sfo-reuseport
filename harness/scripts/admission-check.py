#!/usr/bin/env python3
"""Check implementation admission for explicit module/change ids.

The checker verifies mandatory proposal/design structure, approval state, and direct change traceability.
It does not replace human or agent reading of the approved documents.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


FORBIDDEN_BROAD_IDS = {"all", "any", "bugfix", "change", "cleanup", "misc", "module", "refactor", "task"}
REQUIRED_DOCS = ("proposal.md", "design.md")
APPROVAL_DOCS = ("proposal.md", "design.md")
TABLE_SEPARATOR_RE = re.compile(r"^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)+\|?\s*$")


def fail(message: str) -> None:
    print(f"admission-check: {message}", file=sys.stderr)
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


def validate_change_id(change_id: str) -> None:
    if change_id.lower() in FORBIDDEN_BROAD_IDS:
        fail(f"change_id is too broad: {change_id}")
    if not re.fullmatch(r"[A-Za-z][A-Za-z0-9_.-]{2,63}", change_id):
        fail(f"change_id must be a stable 3-64 character id: {change_id}")


def require_approved(path: Path, module: str, version: str, submodule: str | None = None) -> None:
    data = front_matter(read_text(path), path)
    if data.get("module") != module:
        fail(f"{path} module mismatch: expected {module}, got {data.get('module', '<missing>')}")
    if data.get("version") != version:
        fail(f"{path} version mismatch: expected {version}, got {data.get('version', '<missing>')}")
    if submodule and data.get("submodule") not in {None, "", submodule}:
        fail(f"{path} submodule mismatch: expected {submodule}, got {data.get('submodule')}")
    if data.get("status") != "approved":
        fail(f"{path} is not approved")
    if not data.get("approved_by") or not data.get("approved_at"):
        fail(f"{path} approval metadata is incomplete")


def normalize_column(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")


def split_table_row(line: str) -> list[str]:
    parts = [part.strip() for part in line.strip().split("|")]
    if parts and parts[0] == "":
        parts = parts[1:]
    if parts and parts[-1] == "":
        parts = parts[:-1]
    return parts


def table_rows_after_heading(text: str, heading: str, path: Path) -> list[dict[str, str]]:
    heading_pattern = re.compile(rf"(?m)^##\s+{re.escape(heading)}\s*$")
    match = heading_pattern.search(text)
    if not match:
        fail(f"{path} missing required section: ## {heading}")

    lines = text[match.end() :].splitlines()
    table_start = None
    for index, line in enumerate(lines):
        if re.match(r"^##\s+", line):
            break
        if "|" in line and index + 1 < len(lines) and TABLE_SEPARATOR_RE.match(lines[index + 1]):
            table_start = index
            break
    if table_start is None:
        fail(f"{path} section ## {heading} missing required table")

    headers = [normalize_column(cell) for cell in split_table_row(lines[table_start])]
    rows: list[dict[str, str]] = []
    for line in lines[table_start + 2 :]:
        if not line.strip() or not line.lstrip().startswith("|"):
            break
        values = split_table_row(line)
        row = {header: values[pos].strip() if pos < len(values) else "" for pos, header in enumerate(headers)}
        rows.append(row)
    if not rows:
        fail(f"{path} section ## {heading} has no data rows")
    return rows


def require_table_change(
    path: Path,
    text: str,
    heading: str,
    change_id: str,
    required_columns: tuple[str, ...],
    required_values: tuple[str, ...],
) -> dict[str, str]:
    rows = table_rows_after_heading(text, heading, path)
    available_columns = set(rows[0])
    missing_columns = [column for column in required_columns if column not in available_columns]
    if missing_columns:
        fail(f"{path} ## {heading} missing columns: {', '.join(missing_columns)}")

    matches = [row for row in rows if row.get("change_id") == change_id]
    if not matches:
        fail(f"change_id {change_id} missing from {path} ## {heading} change_id column")
    if len(matches) > 1:
        fail(f"change_id {change_id} appears multiple times in {path} ## {heading}")

    row = matches[0]
    empty_values = [column for column in required_values if not row.get(column)]
    if empty_values:
        fail(f"change_id {change_id} in {path} ## {heading} has empty fields: {', '.join(empty_values)}")
    return row


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=".")
    parser.add_argument("--version", required=True)
    parser.add_argument("--module", required=True)
    parser.add_argument("--submodule")
    parser.add_argument("--change-id", action="append", required=True, dest="change_ids")
    args = parser.parse_args()

    packet = Path(args.root) / "docs" / "versions" / args.version / "modules" / args.module
    if args.submodule:
        packet = packet / args.submodule
    for name in REQUIRED_DOCS:
        if not (packet / name).exists():
            fail(f"missing required file: {packet / name}")
    for name in APPROVAL_DOCS:
        require_approved(packet / name, args.module, args.version, args.submodule)

    docs = {name: read_text(packet / name) for name in REQUIRED_DOCS}
    for change_id in args.change_ids:
        validate_change_id(change_id)
        require_table_change(
            packet / "proposal.md",
            docs["proposal.md"],
            "Proposal Items",
            change_id,
            ("proposal_id", "change_id", "outcome", "success_evidence"),
            ("proposal_id", "outcome", "success_evidence"),
        )
        require_table_change(
            packet / "design.md",
            docs["design.md"],
            "Directly Mapped Change Items",
            change_id,
            ("change_id", "proposal_id", "design_coverage", "scope_paths"),
            ("proposal_id", "design_coverage", "scope_paths"),
        )

    print("admission-check: passed")
    print("Implementation admission still requires reading the approved docs before code edits.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
