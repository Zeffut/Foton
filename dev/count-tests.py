#!/usr/bin/env python3
"""Record how many unit tests the workspace has, and across how many targets.

The website quotes this number and cannot compile the workspace to find it out,
so it is written here and committed, the same way `dev/parity-gaps.txt` is.

    python3 dev/count-tests.py            # rewrite dev/test-counts.json
    python3 dev/count-tests.py --check    # fail when the committed file is stale

`cargo test -- --list` needs the test binaries, so run this after a build.
"""

import argparse
import json
import pathlib
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
OUTPUT = REPO / "dev" / "test-counts.json"


def measure():
    result = subprocess.run(
        ["cargo", "test", "--workspace", "--", "--list"],
        cwd=REPO, capture_output=True, text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit("cargo test --list failed; build the workspace first")
    tests = sum(1 for line in result.stdout.splitlines() if line.endswith(": test"))
    # cargo streams its `Running <target>` progress lines to stderr, and
    # capture_output keeps the two apart -- counting targets in stdout alone
    # would always find zero.
    targets = sum(1 for line in (result.stdout + result.stderr).splitlines()
                  if line.strip().startswith("Running "))
    return {"unit_tests": tests, "targets": targets}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="fail instead of rewriting when the file is stale")
    args = parser.parse_args()

    fresh = measure()
    rendered = json.dumps(fresh, indent=2) + "\n"

    if args.check:
        if not OUTPUT.is_file():
            print(f"{OUTPUT.relative_to(REPO)} is missing; run python3 dev/count-tests.py",
                  file=sys.stderr)
            return 1
        if OUTPUT.read_text(encoding="utf-8") != rendered:
            # Say both numbers. "Stale" alone is unactionable when the machine
            # that disagrees is a CI runner nobody can put a shell on: it took
            # a whole build to learn only that two machines counted
            # differently, and not by how much.
            try:
                committed = json.loads(OUTPUT.read_text(encoding="utf-8"))
            except json.JSONDecodeError:
                committed = {}
            print(f"{OUTPUT.relative_to(REPO)} is stale; run python3 dev/count-tests.py",
                  file=sys.stderr)
            print(f"  committed: {committed.get('unit_tests', '?')} tests across "
                  f"{committed.get('targets', '?')} targets", file=sys.stderr)
            print(f"  measured here: {fresh['unit_tests']} tests across "
                  f"{fresh['targets']} targets", file=sys.stderr)
            return 1
        return 0

    OUTPUT.write_text(rendered, encoding="utf-8")
    print(f"wrote {OUTPUT.relative_to(REPO)}: "
          f"{fresh['unit_tests']} tests across {fresh['targets']} targets")
    return 0


if __name__ == "__main__":
    sys.exit(main())
