#!/usr/bin/env python3
"""Render the Foton website from hand-written fragments and generated facts.

    python3 dev/gen-site.py              # build into site/dist/
    python3 dev/gen-site.py --check      # build into a temp dir and verify

The prose lives in `site/content/<lang>/`. Every `{{ hole }}` in it is filled
from `dev/facts.py`, and a hole with no fact behind it fails the build -- the
site cannot ship a number nobody generated, and it cannot ship `{{ }}` either.

The site is bilingual. Each edition lives at its own prefix (`/en/`, `/fr/`);
the bare root is not itself an edition, it is a 302 redirect (see
`vercel.json`) that reads the visitor's `Accept-Language` -- see the module
docstring for `strings()` on why a missing translation stops the build rather
than silently falling back to English.
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

# code, url prefix, the label its own switcher entry shows
LANGUAGES = [("en", "en", "English"), ("fr", "fr", "Français")]

# slug, output path within an edition, nav label key (None = not in the nav)
PAGES = [
    ("index", "index.html", None),
    ("start", "start/index.html", "nav_start"),
    ("configuration", "configuration/index.html", "nav_configuration"),
    ("contributing", "contributing/index.html", "nav_contributing"),
]

STRINGS = {
    "en": {
        "nav_start": "Get started",
        "nav_configuration": "Configuration",
        "nav_contributing": "Contributing",
        "chip": "PRE-ALPHA",
        "switch": "Français",
        "footer_source": "Source",
        "title_index": "Foton — a Minecraft server that refuses to guess",
        "desc_index": "An independent Minecraft Java Edition server written in Rust, built against the decompiled vanilla source.",
        "title_start": "Get started — Foton",
        "desc_start": "Install Foton from a release binary, the Docker image or source, and boot your first world.",
        "title_configuration": "Configuration — Foton",
        "desc_configuration": "Every Foton configuration key, with its type, default and range, generated from the schemas the server validates against.",
        "title_contributing": "Contributing — Foton",
        "desc_contributing": "How Foton is built: the behavior mechanism, the engineering rules, and the checks a change has to clear.",
    },
    "fr": {
        "nav_start": "Démarrer",
        "nav_configuration": "Configuration",
        "nav_contributing": "Contribuer",
        "chip": "PRÉ-ALPHA",
        "switch": "English",
        "footer_source": "Code source",
        "title_index": "Foton — un serveur Minecraft qui refuse de deviner",
        "desc_index": "Un serveur Minecraft Java Edition indépendant, écrit en Rust, construit contre la source vanilla décompilée.",
        "title_start": "Démarrer — Foton",
        "desc_start": "Installer Foton depuis un binaire, l'image Docker ou les sources, et lancer son premier monde.",
        "title_configuration": "Configuration — Foton",
        "desc_configuration": "Chaque clé de configuration de Foton, avec son type, sa valeur par défaut et sa plage, générée depuis les schémas contre lesquels le serveur valide.",
        "title_contributing": "Contribuer — Foton",
        "desc_contributing": "Comment Foton est construit : le mécanisme de comportement, les règles d'ingénierie, et la barre qu'un changement doit passer.",
    },
}


def strings(lang):
    """The string table for one language. A missing key stops the build."""
    table = STRINGS.get(lang)
    if table is None:
        raise SystemExit(f"no string table for language {lang!r}")

    class Table(dict):
        def __missing__(self, key):
            raise SystemExit(f"{lang}: no translation for {key!r}")

    return Table(table)


def url(prefix, target=""):
    """Root-relative URL of a page within one edition, always as a directory
    (trailing slash) to match `vercel.json`'s `trailingSlash: true`."""
    path = "/".join(part for part in (prefix, target) if part)
    if not path:
        return "/"
    if not path.endswith("/"):
        path += "/"
    return "/" + path


def fragment(lang, slug):
    return CONTENT / lang / f"{slug}.html"


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


def nav_html(lang, prefix, current_slug):
    """Derived from PAGES, so it cannot link a page the build does not emit,
    and never leaves its own edition."""
    text = strings(lang)
    parts = []
    for slug, target, label_key in PAGES:
        if not label_key:
            continue
        href = url(prefix, target[: -len("index.html")])
        mark = ' class="here"' if slug == current_slug else ""
        parts.append(f'<a href="{href}"{mark}>{text[label_key]}</a>')
    return "".join(parts)


def switcher_html(lang, target):
    """A link to the same page in the other edition."""
    other = [entry for entry in LANGUAGES if entry[0] != lang][0]
    other_code, other_prefix, _label = other
    href = url(other_prefix, target[: -len("index.html")])
    return (f'<a class="lang" href="{href}" hreflang="{other_code}" '
            f'lang="{other_code}">{strings(lang)["switch"]}</a>')


def build(out_dir):
    """Renders every page in every language. Returns what was written."""
    out_dir.mkdir(parents=True, exist_ok=True)
    values = facts.all()
    shell = read("_shell.html")
    written = []

    for lang, prefix, _label in LANGUAGES:
        text = strings(lang)
        for slug, target, _label_key in PAGES:
            path_in_content = fragment(lang, slug)
            if not path_in_content.is_file():
                raise SystemExit(f"missing fragment: {path_in_content}")
            body = fill(path_in_content.read_text(encoding="utf-8"), values,
                        f"site/content/{lang}/{slug}.html")
            page = fill(shell, {
                **values,
                "body": body,
                "lang": lang,
                "title": text[f"title_{slug}"],
                "description": text[f"desc_{slug}"],
                "nav": nav_html(lang, prefix, slug),
                "switcher": switcher_html(lang, target),
                "chip": text["chip"],
                "footer_source": text["footer_source"],
                "home": url(prefix),
            }, "site/content/_shell.html")
            path = out_dir / prefix / target
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
