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
import datetime
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
import config_schema as cs  # noqa: E402

HOLE = re.compile(r"\{\{\s*([a-z0-9_]+)\s*\}\}")

# code, url prefix, the label its own switcher entry shows
LANGUAGES = [("en", "en", "English"), ("fr", "fr", "Français")]

# slug, output path within an edition, nav label key (None = not in the nav)
PAGES = [
    ("index", "index.html", None),
    ("start", "start/index.html", "nav_start"),
    ("configuration", "configuration/index.html", "nav_configuration"),
    ("contributing", "contributing/index.html", "nav_contributing"),
    ("bugs", "bugs/index.html", "nav_bugs"),
]

STRINGS = {
    "en": {
        "nav_start": "Get started",
        "nav_configuration": "Configuration",
        "nav_contributing": "Contributing",
        "nav_bugs": "Reports",
        "chip": "PRE-ALPHA",
        "switch": "Français",
        "footer_source": "Source",
        "footer_fork": "Forked from",
        "title_index": "Foton — the same Minecraft, a faster server",
        "desc_index": "A Minecraft Java Edition server that keeps up when your players explore, settings you can read, and nothing else to install.",
        "title_start": "Get started — Foton",
        "desc_start": "Install Foton from a release binary, the Docker image or source, and boot your first world.",
        "title_configuration": "Configuration — Foton",
        "desc_configuration": "Every Foton configuration key, with its type, default and range, generated from the schemas the server validates against.",
        "title_contributing": "Contributing — Foton",
        "desc_contributing": "How Foton is built: the behavior mechanism, the engineering rules, and the checks a change has to clear.",
        "title_bugs": "Reports — Foton",
        "desc_bugs": "Every bug players have filed from inside the game, as they filed it, with what has been fixed since.",
        "report_open": "open",
        "report_fixed": "fixed",
        "report_closed": "not a defect",
        "report_none": "No reports yet. The first one will appear here on its own.",
        "report_in": "in",
        "config_key": "Key",
        "config_type": "Type",
        "config_default": "Default",
        "config_limits": "Limits",
        "config_meaning": "Meaning",
        "config_required": "required",
        "config_shapes": "Structures",
        "config_value_types": "Value types",
        "config_name": "Name",
        "config_accepts": "Accepts",
    },
    "fr": {
        "nav_start": "Démarrer",
        "nav_configuration": "Configuration",
        "nav_contributing": "Contribuer",
        "nav_bugs": "Rapports",
        "chip": "PRÉ-ALPHA",
        "switch": "English",
        "footer_source": "Code source",
        "footer_fork": "Fork de",
        "title_index": "Foton — le même Minecraft, un serveur plus rapide",
        "desc_index": "Un serveur Minecraft Java Edition qui suit la cadence quand vos joueurs explorent, des réglages lisibles, et rien d'autre à installer.",
        "title_start": "Démarrer — Foton",
        "desc_start": "Installer Foton depuis un binaire, l'image Docker ou les sources, et lancer son premier monde.",
        "title_configuration": "Configuration — Foton",
        "desc_configuration": "Chaque clé de configuration de Foton, avec son type, sa valeur par défaut et sa plage, générée depuis les schémas contre lesquels le serveur valide.",
        "title_contributing": "Contribuer — Foton",
        "desc_contributing": "Comment Foton est construit : le mécanisme de comportement, les règles d'ingénierie, et la barre qu'un changement doit passer.",
        "title_bugs": "Rapports — Foton",
        "desc_bugs": "Tous les bugs signalés par les joueurs depuis le jeu, tels qu'ils ont été déposés, et ce qui a été corrigé depuis.",
        "report_open": "ouvert",
        "report_fixed": "corrigé",
        "report_closed": "pas un défaut",
        "report_none": "Aucun rapport pour l'instant. Le premier apparaîtra ici tout seul.",
        "report_in": "dans",
        "config_key": "Clé",
        "config_type": "Type",
        "config_default": "Défaut",
        "config_limits": "Limites",
        "config_meaning": "Rôle",
        "config_required": "requis",
        "config_shapes": "Structures",
        "config_value_types": "Types de valeur",
        "config_name": "Nom",
        "config_accepts": "Accepte",
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


def escape(text):
    """HTML-escapes a report's own words.

    Everything on this page below the prose was typed by a player into a game
    client, so it is untrusted input rendered into a public page. Escaping is
    not a nicety here.
    """
    return (
        str(text)
        .replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def config_part_html(part):
    """One nested configuration type part, as safe HTML."""
    kind, value = part
    if kind == "ref":
        return f"<code>{escape(value)}</code>"
    if kind == "enum":
        return " | ".join(f"<code>{escape(item)}</code>" for item in value)
    if kind == "array":
        return f"array of {config_part_html(value)}" if value else "array"
    if kind == "object":
        return "table"
    return escape(value)


def config_type_html(node, root):
    """The schema type for one configuration key, as safe HTML."""
    kind, value = cs.type_parts(node, root)
    if kind == "union":
        return " or ".join(dict.fromkeys(config_part_html(part) for part in value))
    return config_part_html((kind, value))


def config_default_html(node, root):
    """The declared schema default, without inventing a value when absent."""
    value = cs.default_of(node, root)
    if value is None:
        return ""
    if isinstance(value, bool):
        rendered = str(value).lower()
    elif isinstance(value, str) and value == "":
        rendered = '""'
    else:
        rendered = json.dumps(value)
    return f"<code>{escape(rendered)}</code>"


def config_table_html(node, root, text):
    """A table of direct keys in one schema object."""
    rows = []
    for name, prop, required in cs.rows(node, root):
        description = cs.resolve(prop, root).get("description") or ""
        required_html = f' <span class="required">{text["config_required"]}</span>' if required else ""
        rows.append(
            "<tr>"
            f"<td><code>{escape(name)}</code>{required_html}</td>"
            f"<td>{config_type_html(prop, root)}</td>"
            f"<td>{config_default_html(prop, root)}</td>"
            f"<td>{escape(cs.limits(prop, root))}</td>"
            f"<td>{escape(description)}</td>"
            "</tr>"
        )
    if not rows:
        return ""
    header = "".join(f"<th>{text[key]}</th>" for key in (
        "config_key", "config_type", "config_default", "config_limits", "config_meaning"
    ))
    return f'<div class="tablewrap"><table><thead><tr>{header}</tr></thead><tbody>{"".join(rows)}</tbody></table></div>'


def config_sections_html(node, root, title, depth, text):
    """Tables and nested TOML sections for one schema object."""
    out = [config_table_html(node, root, text)]
    for name, prop in cs.subsections(node, root):
        heading = f"{title}.{name}" if title else name
        description = cs.resolve(prop, root).get("description")
        out.append(f'<h{depth}><code>[{escape(heading)}]</code></h{depth}>')
        if description:
            out.append(f"<p>{escape(description)}</p>")
        out.append(config_sections_html(prop, root, heading, min(depth + 1, 6), text))
    return "".join(out)


def config_definitions_html(schema, text):
    """Named schema structures and scalar types referenced by configuration keys."""
    definitions = schema.get("definitions") or {}
    shapes = {
        name: node for name, node in definitions.items()
        if node.get("type") == "object" and node.get("properties")
    }
    out = []
    if shapes:
        out.append(f'<h3>{text["config_shapes"]}</h3>')
        for name, node in shapes.items():
            out.append(f"<h4><code>{escape(name)}</code></h4>")
            out.append(config_sections_html(node, schema, name, 5, text))
    scalars = {name: node for name, node in definitions.items() if name not in shapes}
    if scalars:
        rows = []
        for name, node in scalars.items():
            accepts = config_type_html(node, schema)
            if node.get("pattern"):
                accepts += f" {escape(node['pattern'])}"
            description = node.get("description") or ""
            rows.append(
                "<tr>"
                f"<td><code>{escape(name)}</code></td>"
                f"<td>{accepts}</td>"
                f"<td>{escape(description)}</td>"
                "</tr>"
            )
        header = "".join(f"<th>{text[key]}</th>" for key in (
            "config_name", "config_accepts", "config_meaning"
        ))
        out.append(f'<h3>{text["config_value_types"]}</h3>')
        out.append(f'<div class="tablewrap"><table><thead><tr>{header}</tr></thead><tbody>{"".join(rows)}</tbody></table></div>')
    return "".join(out)


def configuration_reference_html(text):
    """The configuration reference rendered from the shipped JSON schemas."""
    files = []
    for toml_name, schema_name, title, blurb in cs.FILES:
        schema = cs.load(schema_name)
        description = schema.get("description", blurb)
        files.append(
            '<section class="config-file">'
            f"<h2><code>{escape(toml_name)}</code></h2>"
            f"<p>{escape(description)}.</p>"
            f"{config_sections_html(schema, schema, '', 3, text)}"
            f"{config_definitions_html(schema, text)}"
            "</section>"
        )
    return "".join(files)


def reports_html(text):
    """Renders every filed report, newest first.

    The report's own words are shown as they were written, in the language
    their author used. Translating them would be rewriting them, and the point
    of the page is that it says what the tester said.
    """
    # A fixed report is a closed subject: the count says how many there were,
    # and the page keeps its length for what still needs attention.
    reports = [r for r in facts.bug_reports()
               if r.get("status") not in ("fixed", "closed")]
    if not reports:
        return f'<p class="rest">{escape(text["report_none"])}</p>'

    rows = []
    for report in reports:
        status = report.get("status", "open")
        label = text.get(f"report_{status}", status)
        when = datetime.datetime.fromtimestamp(
            report.get("at", 0), datetime.timezone.utc
        ).strftime("%Y-%m-%d")
        world = report.get("world", "").split(":")[-1].replace("_", " ")
        note = report.get("note")
        rows.append(
            f'<article class="report is-{escape(status)}">'
            f'<header><span class="report-no">#{escape(report.get("number", "?"))}</span>'
            f'<span class="report-state">{escape(label)}</span>'
            f'<span class="report-cat">{escape(report.get("category", ""))}</span></header>'
            f"<p>{escape(report.get('description', ''))}</p>"
            f'<footer class="rest">{escape(report.get("player", ""))} — {escape(when)}'
            f' — {escape(world)} — {escape(text["report_in"])} '
            f'{escape(report.get("version", ""))}</footer>'
            + (f'<p class="report-note">{escape(note)}</p>' if note else "")
            + "</article>"
        )
    return '<div class="reports">' + "".join(rows) + "</div>"


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
            body = fill(path_in_content.read_text(encoding="utf-8"),
                        {**values, "reports": reports_html(text),
                         "configuration_reference": configuration_reference_html(text)},
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
                "footer_fork": text["footer_fork"],
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

# Paths the host serves at request time, which the build cannot emit and must
# not be asked to. Kept to exact prefixes rather than a pattern, so an
# exemption can never widen into "stop checking links".
PLATFORM_PATHS = ("/_vercel/",)


def check_links(out_dir, written):
    """Every root-relative link must point at something the build emitted."""
    emitted = {("/" + p.relative_to(out_dir).as_posix()) for p in written}
    problems = []
    for path in written:
        if path.suffix != ".html":
            continue
        for target in LINK.findall(path.read_text(encoding="utf-8")):
            if target.startswith(PLATFORM_PATHS):
                continue
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
