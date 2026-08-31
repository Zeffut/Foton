#!/usr/bin/env python3
"""Every fact the website is allowed to state, read from the repository.

The site's prose is written by hand and its facts are not. A page asks for a
hole by name and gets the value the repository actually holds -- so a version
bump, a new behavior or a renamed config key reaches the site without anyone
retyping it, and a fact that stops existing fails the build instead of going
stale in public.
"""

import importlib.util
import json
import pathlib
import re

REPO = pathlib.Path(__file__).resolve().parent.parent
DEV = REPO / "dev"


def _load_hyphenated(name, filename):
    """Imports a dev script whose filename is not a valid module name."""
    spec = importlib.util.spec_from_file_location(name, DEV / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


coverage = _load_hyphenated("coverage", "coverage.py")


def _read(path, what):
    if not path.is_file():
        raise SystemExit(f"cannot state {what}: {path} is missing")
    return path.read_text(encoding="utf-8")


def _read_json(path, what):
    """Reads and parses JSON, turning a truncated or empty file into the same
    clear, non-zero-exit failure as a missing one instead of a raw traceback."""
    text = _read(path, what)
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        raise SystemExit(f"cannot state {what}: {path} is not valid JSON ({exc})") from exc


def version():
    cargo = _read(REPO / "Cargo.toml", "the version")
    match = re.search(r'^version = "([^"]+)"', cargo, re.M)
    if not match:
        raise SystemExit("cannot state the version: no version in Cargo.toml")
    return match.group(1)


def protocol():
    path = REPO / "foton-registry" / "build_assets" / "packets.json"
    packets = _read_json(path, "the protocol")
    if "version" not in packets:
        raise SystemExit(f"cannot state the protocol: no 'version' key in {path}")
    return str(packets["version"])


def inworld_scripts():
    return len(list(DEV.glob("*-test.sh")))


def test_counts():
    path = DEV / "test-counts.json"
    data = _read_json(path, "the test count")
    for key in ("unit_tests", "targets"):
        if key not in data:
            raise SystemExit(f"cannot state the test count: no {key!r} key in {path}")
    return data


def bug_reports():
    """Every player-filed report the repository holds, newest first.

    The file is append-only and written by the site's own ingest function, so
    a report reaches the page by being committed -- the same rule every other
    fact on the site follows. A malformed line stops the build rather than
    quietly shortening the list: a report that vanishes is worse than one that
    breaks a deploy, because nobody notices the first.
    """
    path = REPO / "dev" / "bug-reports.jsonl"
    if not path.is_file():
        return []
    reports = []
    for number, line in enumerate(_read(path, "the bug reports").splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            reports.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise SystemExit(f"bug-reports.jsonl line {number} is not valid JSON ({exc})") from exc
    reports.sort(key=lambda report: report.get("at", 0), reverse=True)
    return reports


def bug_categories():
    """The categories the in-game form offers.

    Read from a committed file rather than from the enum itself: the deploy
    image has no Rust sources at all -- `.vercelignore` drops `*.rs` -- so a
    build that reached for one could only ever fail in production. The file is
    generated from `BugCategory::label` and a test refuses to pass when the two
    disagree, which is the same shape as `dev/parity-gaps.txt`.
    """
    labels = _read_json(REPO / "dev" / "bug-categories.json", "the bug categories")
    if not isinstance(labels, list) or not labels:
        raise SystemExit("cannot state the bug categories: the list is empty")
    return labels


def _missing_list(names):
    """Comma-joins missing class names -- or says "none" outright once a
    category is fully covered, so the fact never renders an empty sentence."""
    return ", ".join(names) or "none"


def all():
    """Every hole name a template may use, as strings, ready to substitute."""
    ver = version()
    if "+mc" not in ver:
        raise SystemExit(f"cannot state the Minecraft target: version is {ver!r}")
    cov = coverage.counts()
    tests = test_counts()

    out = {
        "version": ver,
        "mc_target": ver.split("+mc", 1)[1],
        "protocol": protocol(),
        "unit_tests": f"{tests['unit_tests']:,}".replace(",", " "),
        "test_targets": str(tests["targets"]),
        "inworld_scripts": str(inworld_scripts()),
    }
    for kind in ("blocks", "items", "entities"):
        entry = cov[kind]
        percent = round(100 * entry["covered"] / max(entry["total"], 1))
        out[f"{kind}_covered"] = str(entry["covered"])
        out[f"{kind}_total"] = str(entry["total"])
        out[f"{kind}_missing"] = str(len(entry["missing"]))
        out[f"{kind}_percent"] = str(percent)
        out[f"{kind}_missing_list"] = _missing_list(entry["missing"])
    reports = bug_reports()
    out["reports_total"] = str(len(reports))
    out["reports_open"] = str(sum(1 for r in reports if r.get("status") == "open"))
    out["reports_fixed"] = str(sum(1 for r in reports if r.get("status") == "fixed"))
    out["bug_categories"] = ", ".join(bug_categories())
    for name, hole in (("_scene-island.svg", "scene_island"),
                       ("_scene-crates.svg", "scene_crates")):
        out[hole] = _read(REPO / "site" / "content" / name, f"the {hole} scene").strip()
    return out


if __name__ == "__main__":
    for key, value in sorted(all().items()):
        print(f"{key:24} {value}")
