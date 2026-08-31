#!/usr/bin/env python3
"""Reading of the shipped JSON config schemas, shared by every renderer.

The schemas in `package-content/` are the only description of the config format
that is checked against the code. `dev/gen-config-docs.py` turns them into
Markdown and `dev/gen-site.py` turns them into HTML; both read them through
here, so the two references cannot describe different things.

Type information comes back as structure, not as formatted text -- a Markdown
link and an HTML anchor are the caller's problem.
"""

import json
import pathlib

REPO = pathlib.Path(__file__).resolve().parent.parent
CONTENT = REPO / "package-content"

FILES = [
    ("config.toml", "config.schema.json", "Server", "server settings and logging"),
    ("worlds.toml", "worlds.schema.json", "Worlds", "dimensions, domains and storage"),
    ("groups.toml", "groups.schema.json", "Permissions", "groups and permission rules"),
]

# Bounds that only say "a 32- or 64-bit integer" carry no information for a
# reader, so they are not reported as a range.
FULL_WIDTH = {(-(2**31), 2**31 - 1), (-(2**63), 2**63 - 1)}


def load(schema_name):
    path = CONTENT / schema_name
    if not path.is_file():
        raise SystemExit(f"missing schema: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def resolve(node, root):
    """Follows a local `$ref` until the node is a real subschema."""
    seen = 0
    while isinstance(node, dict) and "$ref" in node:
        ref = node["$ref"]
        if not ref.startswith("#/"):
            return node
        target = root
        for part in ref[2:].split("/"):
            target = target[part]
        node = target
        seen += 1
        if seen > 16:
            raise RuntimeError(f"cyclic $ref at {ref}")
    return node


def ref_name(node):
    """Definition name a property points at, when it points at one."""
    ref = node.get("$ref", "") if isinstance(node, dict) else ""
    return ref.rsplit("/", 1)[-1] if ref.startswith("#/definitions/") else None


def type_parts(node, root):
    """The type of one property, as structure a renderer can format."""
    name = ref_name(node)
    if name:
        return ("ref", name)
    node = resolve(node, root)
    if "enum" in node:
        return ("enum", list(node["enum"]))
    for key in ("oneOf", "anyOf"):
        if key in node:
            # Deduplicated by equality, not by hashing: an ("enum", [...])
            # part holds a list, so dict.fromkeys would raise here.
            parts = []
            for member in node[key]:
                part = type_parts(member, root)
                if part not in parts:
                    parts.append(part)
            return ("union", parts)
    kind = node.get("type", "")
    if kind == "array":
        inner = node.get("items")
        return ("array", type_parts(inner, root) if inner else None)
    if kind == "object":
        return ("object", None)
    return ("scalar", kind or "any")


def limits(node, root):
    """Range and format constraints, as a short phrase."""
    node = resolve(node, root)
    out = []
    if (node.get("minimum"), node.get("maximum")) in FULL_WIDTH:
        pass
    elif "minimum" in node and "maximum" in node:
        out.append(f"{node['minimum']}–{node['maximum']}")
    elif "minimum" in node:
        out.append(f"≥ {node['minimum']}")
    elif "maximum" in node:
        out.append(f"≤ {node['maximum']}")
    if node.get("format"):
        out.append(node["format"])
    if node.get("minItems"):
        out.append(f"≥ {node['minItems']} item(s)")
    if node.get("uniqueItems"):
        out.append("unique")
    return ", ".join(out)


def default_of(node, root):
    """The declared default, raw. `None` when the schema declares none."""
    return resolve(node, root).get("default")


def _is_subsection(prop, root):
    resolved = resolve(prop, root)
    return (not ref_name(prop)
            and resolved.get("type") == "object"
            and bool(resolved.get("properties")))


def rows(node, root):
    """Properties belonging in this node's table, with their required flag."""
    node = resolve(node, root)
    required = set(node.get("required") or [])
    for name, prop in (node.get("properties") or {}).items():
        if _is_subsection(prop, root):
            continue
        yield name, prop, name in required


def subsections(node, root):
    """Properties rendered as a section of their own."""
    node = resolve(node, root)
    for name, prop in (node.get("properties") or {}).items():
        if _is_subsection(prop, root):
            yield name, prop
