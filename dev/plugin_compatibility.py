#!/usr/bin/env python3
"""Combine distinct plugin compatibility evidence into one JSON report."""

import argparse
import json
import pathlib

REQUIRED_FIXTURE_SUITES = {
    "cross-call-alias",
    "dependency",
    "event",
    "library",
    "lifecycle",
    "malformed-library",
    "paper",
    "replacement",
}


def validate_fixture_report(report):
    """Require complete, causal evidence for the executable fixture corpus."""
    if report.get("schema_version") != 2:
        raise ValueError("fixture report schema_version must be 2")
    suites = report.get("suites")
    if not isinstance(suites, list):
        raise ValueError("fixture report suites must be a list")
    names = {suite.get("suite") for suite in suites}
    missing = REQUIRED_FIXTURE_SUITES - names
    if missing:
        raise ValueError(f"missing fixture suites: {', '.join(sorted(missing))}")
    extra = names - REQUIRED_FIXTURE_SUITES
    if extra:
        raise ValueError(f"unexpected fixture suites: {', '.join(sorted(extra))}")

    totals = {key: 0 for key in ("discovered", "loaded", "enabled", "rejected")}
    for suite in suites:
        name = suite["suite"]
        outcomes = suite.get("outcomes")
        if not isinstance(outcomes, list):
            raise ValueError(f"fixture suite {name} has no outcomes")
        observed = {
            "discovered": len(outcomes),
            "loaded": sum(bool(row.get("loaded")) for row in outcomes),
            "enabled": sum(bool(row.get("enabled")) for row in outcomes),
            "rejected": sum(bool(row.get("rejected")) for row in outcomes),
        }
        for outcome in outcomes:
            if outcome.get("suite") != name or outcome.get("discovered") is not True:
                raise ValueError(f"fixture suite {name} has an invalid discovered outcome")
            loaded = outcome.get("loaded") is True
            enabled = outcome.get("enabled") is True
            rejected = outcome.get("rejected") is True
            if enabled and not loaded:
                raise ValueError(f"fixture {outcome.get('fixture')} enabled without loading")
            if rejected == enabled:
                raise ValueError(f"fixture {outcome.get('fixture')} has inconsistent status")
            expected_phase = "enabled" if enabled else ("enable" if loaded else "load")
            if outcome.get("phase") != expected_phase:
                raise ValueError(f"fixture {outcome.get('fixture')} has inconsistent phase")
            reason = outcome.get("reason")
            if rejected and (not isinstance(reason, str) or not reason.strip()):
                raise ValueError(
                    f"fixture {outcome.get('fixture')} rejection lacks a causal reason")
            if enabled and reason is not None:
                raise ValueError(f"enabled fixture {outcome.get('fixture')} has a reason")
        for key, count in observed.items():
            if suite.get(key) != count:
                raise ValueError(f"fixture suite {name} has inconsistent {key} count")
            totals[key] += count

    if report.get("aggregate") != totals:
        raise ValueError("fixture aggregate does not match suite outcomes")


def combine(api_report, fixture_report):
    """Keep binary, ceiling, event and executable fixture evidence separate."""
    validate_fixture_report(fixture_report)
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
