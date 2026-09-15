#!/usr/bin/env python3
"""Combine distinct plugin compatibility evidence into one JSON report."""

import argparse
import json
import pathlib


def combine(api_report, fixture_report):
    """Keep binary, ceiling, event and executable fixture evidence separate."""
    api = api_report["api"]
    return {
        "binary": {
            "all_members": api["all"],
            "classes": api["classes"],
            "shared_members": api["shared"],
        },
        "ceiling": api_report["ceiling"],
        "events": api_report["events"],
        "fixtures": fixture_report,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--api-report", required=True, type=pathlib.Path)
    parser.add_argument("--fixture-report", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args(argv)

    api_report = json.loads(args.api_report.read_text(encoding="utf-8"))
    fixture_report = json.loads(args.fixture_report.read_text(encoding="utf-8"))
    report = combine(api_report, fixture_report)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")


if __name__ == "__main__":
    main()
