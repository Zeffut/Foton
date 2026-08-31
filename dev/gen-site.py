#!/usr/bin/env python3
"""Render the Foton website from hand-written fragments and generated facts.

    python3 dev/gen-site.py              # build into site/dist/
    python3 dev/gen-site.py --check      # build into a temp dir and verify

The prose lives in `site/content/`. Every `{{ hole }}` in it is filled from
`dev/facts.py`, and a hole with no fact behind it fails the build -- the site
cannot ship a number nobody generated, and it cannot ship `{{ }}` either.
"""

import argparse
import importlib.util
import json
import pathlib
import re
import shutil
import sys
import tempfile

REPO = pathlib.Path(__file__).resolve().parent.parent
DEV = REPO / "dev"
CONTENT = REPO / "site" / "content"
STATIC = REPO / "site" / "static"
DIST = REPO / "site" / "dist"

sys.path.insert(0, str(DEV))
import facts  # noqa: E402

HOLE = re.compile(r"\{\{\s*([a-z0-9_]+)\s*\}\}")

# slug, output path, <title>, meta description, nav label (None = not in the nav)
PAGES = [
    ("index", "index.html",
     "Foton — a Minecraft server that refuses to guess",
     "An independent Minecraft Java Edition server written in Rust, built against the decompiled vanilla source.",
     None),
]


def fill(template, values, where):
    """Substitutes every hole exactly once. Unknown holes stop the build."""
    missing = []

    def swap(match):
        name = match.group(1)
        if name not in values:
            missing.append(name)
            return ""
        return values[name]

    out = HOLE.sub(swap, template)
    if missing:
        raise SystemExit(f"{where}: no fact for {', '.join(sorted(set(missing)))}")
    return out


def read(name):
    path = CONTENT / name
    if not path.is_file():
        raise SystemExit(f"missing fragment: {path}")
    return path.read_text(encoding="utf-8")


def nav_html(current_slug):
    """The nav is derived from PAGES, so it cannot link a page the build does
    not emit -- the link check would fail on it otherwise."""
    parts = []
    for slug, target, _title, _description, label in PAGES:
        if not label:
            continue
        href = "/" + target[: -len("index.html")]
        mark = ' class="here"' if slug == current_slug else ""
        parts.append(f'<a href="{href}"{mark}>{label}</a>')
    return "".join(parts)


def build(out_dir):
    """Renders every page into `out_dir`. Returns what was written."""
    out_dir.mkdir(parents=True, exist_ok=True)
    values = facts.all()
    shell = read("_shell.html")
    written = []

    for slug, target, title, description, _label in PAGES:
        body = fill(read(f"{slug}.html"), values, f"site/content/{slug}.html")
        page = fill(shell, {**values, "body": body, "title": title,
                            "description": description,
                            "nav": nav_html(slug)},
                    "site/content/_shell.html")
        path = out_dir / target
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(page, encoding="utf-8")
        written.append(path)

    if STATIC.is_dir():
        for asset in STATIC.iterdir():
            if asset.is_file():
                shutil.copy2(asset, out_dir / asset.name)
                written.append(out_dir / asset.name)
    return written


LINK = re.compile(r'(?:href|src)="(/[^"#?]*)')


def check_links(out_dir, written):
    """Every root-relative link must point at something the build emitted."""
    emitted = {("/" + p.relative_to(out_dir).as_posix()) for p in written}
    problems = []
    for path in written:
        if path.suffix != ".html":
            continue
        for target in LINK.findall(path.read_text(encoding="utf-8")):
            candidates = {target, target.rstrip("/") + "/index.html",
                          target + "index.html"}
            if not candidates & emitted:
                problems.append(f"{path.name}: dead link {target}")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="build into a temporary directory and verify only")
    args = parser.parse_args()

    if args.check:
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            problems = check_links(out, build(out))
        if problems:
            print("\n".join(problems), file=sys.stderr)
            return 1
        print("site builds clean")
        return 0

    if DIST.exists():
        shutil.rmtree(DIST)
    written = build(DIST)
    problems = check_links(DIST, written)
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"wrote {len(written)} files to {DIST.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
