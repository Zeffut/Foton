#!/usr/bin/env python3
"""The player reports that still need work, and what is actually known of each.

`/bug` appends a report to `dev/bug-reports.jsonl` and opens a GitHub issue for
it. The two halves drift apart on purpose: the file only learns that an issue
was closed once the webhook fires and commits the new status back, so a report
resolved minutes ago still reads `open` in any checkout that has not pulled
since. GitHub is the authority -- `REPORTING.md` says so -- and this script
asks it rather than letting a reader trust the stale half.

    python3 dev/reports.py                 # what is still open
    python3 dev/reports.py --all           # every report, whatever its status
    python3 dev/reports.py 14              # one report, in full
    python3 dev/reports.py --category mobs
    python3 dev/reports.py --json          # for a script rather than a person

An issue closed without exactly one of `fixed` / `not-a-bug` is reported as
AMBIGUOUS: the site refuses to publish a resolution it cannot explain, so such
a report is invisible to the player who filed it until a label is applied.

Standard library only, like the rest of `dev/`.
"""

import argparse
import json
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
REPORTS = REPO / "dev" / "bug-reports.jsonl"
REPORT_LABEL = "foton-report"
FIXED_LABEL = "fixed"
NOT_A_BUG_LABEL = "not-a-bug"


def load_reports():
    """Every report, oldest first, as written by the reporting endpoint."""
    if not REPORTS.is_file():
        raise SystemExit(f"{REPORTS.relative_to(REPO)} does not exist")
    reports = []
    for number, line in enumerate(REPORTS.read_text(encoding="utf-8").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            reports.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise SystemExit(f"bug-reports.jsonl line {number} is not valid JSON ({exc})") from exc
    return reports


def issue_states():
    """What GitHub says about each report issue, or None when it cannot say.

    A missing or unauthenticated `gh` is not an error: the committed statuses
    are still worth reading, as long as the caller is told they may be stale.
    """
    try:
        result = subprocess.run(
            ["gh", "issue", "list", "--label", REPORT_LABEL, "--state", "all",
             "--limit", "500", "--json", "number,state,labels,url"],
            capture_output=True, text=True, cwd=REPO, encoding="utf-8",
        )
    except OSError:
        return None
    if result.returncode != 0:
        return None
    try:
        issues = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None
    return {issue["number"]: issue for issue in issues}


def issue_status(issue):
    """The site status GitHub implies, or None when it implies none.

    Mirrors `_issue_status` in `api/report.py`; the two must agree or a report
    shows one thing here and another on the website.
    """
    if issue.get("state", "").lower() == "open":
        return "open"
    labels = {label.get("name") for label in issue.get("labels", [])}
    if FIXED_LABEL in labels and NOT_A_BUG_LABEL not in labels:
        return "fixed"
    if NOT_A_BUG_LABEL in labels and FIXED_LABEL not in labels:
        return "closed"
    return None


def issue_number_of(report):
    """The issue a report belongs to.

    Reports #1-#10 predate the integration and carry no `issue_number`; their
    hand-made issues reuse the report number, which is the same fallback
    `_sync_status` applies in `api/report.py`.
    """
    return report.get("issue_number") or report.get("number")


def resolve(report, states):
    """Pair a report with the truth, and say which source that truth came from."""
    committed = report.get("status", "open")
    if states is None:
        return committed, "committed", None
    issue = states.get(issue_number_of(report))
    if issue is None:
        return committed, "committed", None
    live = issue_status(issue)
    if live is None:
        return "AMBIGUOUS", "github", issue
    return live, "github", issue


def describe(report, status, issue, verbose):
    number = report.get("number")
    category = report.get("category", "?")
    head = f"#{number:>3} [{status:<9}] {category:<9} {report.get('description', '').strip()}"
    if not verbose:
        # One line per report: the list is meant to be scanned, not read.
        return head.replace("\n", " ")[:150]

    position = report.get("position") or []
    where = " ".join(f"{value:.1f}" for value in position) if position else "unknown"
    lines = [
        f"Report #{number}  [{status}]  category: {category}",
        f"  player   : {report.get('player', '?')}",
        f"  version  : {report.get('version', '?')}",
        f"  where    : {report.get('world', '?')} at {where}",
        f"  issue    : {report.get('issue_url') or (issue or {}).get('url') or 'none'}",
        "",
        "  " + report.get("description", "").strip().replace("\n", "\n  "),
    ]
    if report.get("note"):
        lines += ["", f"  note on the site: {report['note']}"]
    return "\n".join(lines)


def main():
    # The reports are French prose. On Windows stdout defaults to cp1252 and
    # every accent raises UnicodeEncodeError halfway through the listing, which
    # is how CONFIGURATION.md once got truncated to nothing.
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("number", nargs="?", type=int,
                        help="show one report in full")
    parser.add_argument("--all", action="store_true",
                        help="include reports already resolved")
    parser.add_argument("--category", help="only this category")
    parser.add_argument("--json", action="store_true",
                        help="machine-readable output")
    args = parser.parse_args()

    reports = load_reports()
    states = issue_states()
    resolved = [(report,) + resolve(report, states) for report in reports]

    if args.number is not None:
        chosen = [row for row in resolved if row[0].get("number") == args.number]
        if not chosen:
            raise SystemExit(f"no report #{args.number}")
    else:
        chosen = resolved
        if not args.all:
            chosen = [row for row in chosen if row[1] != "fixed" and row[1] != "closed"]
        if args.category:
            wanted = args.category.lower()
            chosen = [row for row in chosen if (row[0].get("category") or "").lower() == wanted]

    if args.json:
        print(json.dumps([
            {**report, "status": status, "status_source": source}
            for report, status, source, _ in chosen
        ], ensure_ascii=False, indent=2))
        return 0

    if states is None:
        print("gh unavailable: statuses below are the committed ones and may be stale.\n")

    verbose = args.number is not None
    for report, status, _, issue in chosen:
        print(describe(report, status, issue, verbose))
        if verbose:
            print()

    if not verbose:
        ambiguous = [report["number"] for report, status, _, _ in chosen if status == "AMBIGUOUS"]
        print(f"\n{len(chosen)} report(s) listed.")
        if ambiguous:
            print(f"AMBIGUOUS: {ambiguous} -- closed without exactly one of "
                  f"'{FIXED_LABEL}'/'{NOT_A_BUG_LABEL}', so the site shows no resolution.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
