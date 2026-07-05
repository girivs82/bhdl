#!/usr/bin/env python3
"""Reference T3 ERC policy plugin (docs/spec/ERC.md §2).

Reads one DesignSummary JSON document on stdin, writes one PolicyResponse
JSON document on stdout. Any executable obeying this exchange can be listed
in BHDL_ERC_PLUGINS (colon-separated); findings gate and waive exactly like
built-in rules.

Demonstrates two house rules:
  NAMING-001 (warning)  passive refdes prefix convention: capacitors start
                        with 'c', resistors 'r', inductors 'l'.
  RAIL-001   (info)     a power rail declared without a load budget
                        (`power X = V` with no `@ I`) — the org wants every
                        rail budgeted so ERC016 can gate it.
"""

import json
import sys

PREFIX = {"capacitor": "c", "resistor": "r", "inductor": "l"}


def main() -> None:
    summary = json.load(sys.stdin)
    findings = []

    for inst in summary.get("instances", []):
        cls = inst.get("attributes", {}).get("component_class", "")
        want = PREFIX.get(cls)
        if want and not inst["refdes"].lower().startswith(want):
            findings.append({
                "rule_id": "NAMING-001",
                "severity": "warning",
                "description": (
                    f"{inst['refdes']} is a {cls} but does not follow the "
                    f"house prefix convention ('{want}…')"
                ),
                "fix": f"rename to {want}_{inst['refdes']} or similar",
                "instance": inst["refdes"],
            })

    for net in summary.get("nets", []):
        if net.get("class") == "power" and net.get("budget_a") is None:
            findings.append({
                "rule_id": "RAIL-001",
                "severity": "info",
                "description": (
                    f"rail '{net['name']}' has no declared load budget — "
                    "house rule: every rail declares `@ I` so the budget "
                    "check (ERC016) can gate it"
                ),
                "fix": "declare the rail as `power NAME = V @ I`",
                "net": net["name"],
            })

    json.dump({
        "protocol_version": "1",
        "findings": findings,
        "warnings": [],
    }, sys.stdout)


if __name__ == "__main__":
    main()
