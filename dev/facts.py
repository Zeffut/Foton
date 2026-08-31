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
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
DEV = REPO / "dev"
sys.path.insert(0, str(DEV))


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


def version():
    cargo = _read(REPO / "Cargo.toml", "the version")
    match = re.search(r'^version = "([^"]+)"', cargo, re.M)
    if not match:
        raise SystemExit("cannot state the version: no version in Cargo.toml")
    return match.group(1)


def protocol():
    packets = json.loads(_read(
        REPO / "foton-registry" / "build_assets" / "packets.json", "the protocol"))
    return str(packets["version"])


def inworld_scripts():
    return len(list(DEV.glob("*-test.sh")))


def test_counts():
    return json.loads(_read(DEV / "test-counts.json", "the test count"))


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
        # Reads as a sentence when the gap closes, instead of rendering an
        # empty paragraph and failing the "no empty fact" check on a success.
        out[f"{kind}_missing_list"] = ", ".join(entry["missing"]) or "none"
    return out


if __name__ == "__main__":
    for key, value in sorted(all().items()):
        print(f"{key:24} {value}")
