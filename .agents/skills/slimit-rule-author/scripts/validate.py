#!/usr/bin/env python3
"""Validate SlimIt rule library against rules/schema-v1.json plus lint rules.

Usage: python3 validate.py <rules-dir>   (defaults to ./rules)
Exit 0 = all clean, exit 1 = failures (printed).
"""
import json
import re
import sys
from pathlib import Path

try:
    import jsonschema
except ImportError:
    print("missing dependency: pip install jsonschema", file=sys.stderr)
    sys.exit(2)

try:
    import yaml
except ImportError:
    print("missing dependency: pip install pyyaml", file=sys.stderr)
    sys.exit(2)

PREFIX = {"macos": "macos-", "windows": "win-", "linux": "linux-"}


def main() -> int:
    rules_dir = Path(sys.argv[1] if len(sys.argv) > 1 else "rules")
    schema = json.loads((rules_dir / "schema-v1.json").read_text())
    failures, seen_ids = [], set()

    files = sorted(p for p in rules_dir.rglob("*.yaml") if not p.name.startswith("_"))
    if not files:
        print(f"no rule files found under {rules_dir}")
        return 1

    for path in files:
        rel = path.relative_to(rules_dir)
        try:
            rule = yaml.safe_load(path.read_text())
        except yaml.YAMLError as e:
            failures.append(f"{rel}: YAML parse error: {e}")
            continue

        try:
            jsonschema.validate(rule, schema)
        except jsonschema.ValidationError as e:
            failures.append(f"{rel}: schema: {e.message}")
            continue

        rid, os_name = rule["id"], rule["os"]
        if rid in seen_ids:
            failures.append(f"{rel}: duplicate id {rid}")
        seen_ids.add(rid)
        if not rid.startswith(PREFIX[os_name]):
            failures.append(f"{rel}: id '{rid}' missing prefix '{PREFIX[os_name]}'")

        action = rule["action"]
        if action["kind"] == "command" and not action.get("dry_run"):
            failures.append(f"{rel}: command rule missing dry_run")
        if rule["risk"] == "red" and not rule.get("red_flags"):
            failures.append(f"{rel}: red rule missing red_flags")
        if rule["risk"] == "red" and action["kind"] == "purge-dir":
            failures.append(f"{rel}: red rule must not use purge-dir")

    if failures:
        print(f"{len(failures)} failure(s):")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"OK: {len(files)} rule(s) valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
