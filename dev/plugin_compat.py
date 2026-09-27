#!/usr/bin/env python3
"""Validate a local plugin corpus and emit evidence-backed certification axes.

The manifest records artifacts; runtime results live in a separate JSON input.
Absent artifacts and absent results are never inferred to pass from static API
coverage. JAR paths are operator-local inputs, not redistribution instructions.
"""

import argparse
import hashlib
import json
import pathlib
import re
import sys

AXES = ("linkage", "load", "events", "behavior", "coinstall")
STATUSES = {"pass", "fail", "not_tested"}
SHA256 = re.compile(r"[a-fA-F0-9]{64}\Z")
PAPER_BUILD = re.compile(r"paper:[0-9]+(?:\.[0-9]+)*:[1-9][0-9]*\Z")
FOTON_BUILD = re.compile(r"foton:[a-fA-F0-9]{40}\Z")
UNKNOWN_METADATA = re.compile(r"\b(?:not[\s_-]*(?:verified|recorded|known|available)|"
                              r"unresolved|unknown|missing)\b", re.IGNORECASE)
REQUIRED = ("name", "version", "variant", "sha256", "source", "license",
            "api_version", "paper_build", "dependencies", "scenarios")


def artifact_id(row):
    return f"{row['name']}@{row['version']}#{row['variant']}"


def product_count(rows):
    return len({(row["name"], row["version"]) for row in rows})


def validate_manifest(manifest):
    errors = []
    target = manifest.get("foton_target")
    if not isinstance(target, str) or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)*", target):
        errors.append("foton_target must be a Minecraft version")
    rows = manifest.get("artifacts")
    if not isinstance(rows, list):
        return ["artifacts must be a list"]
    identities = set()
    by_identity = {artifact_id(row): row for row in rows if isinstance(row, dict)
                   and all(isinstance(row.get(key), str) for key in ("name", "version", "variant"))}
    for index, row in enumerate(rows):
        label = f"artifacts[{index}]"
        if not isinstance(row, dict):
            errors.append(f"{label}: expected object")
            continue
        missing = [key for key in REQUIRED if key not in row]
        if missing:
            errors.append(f"{label}: missing {', '.join(missing)}")
            continue
        for key in ("name", "version", "variant", "source", "license",
                    "api_version", "paper_build"):
            if not isinstance(row[key], str) or not row[key].strip():
                errors.append(f"{label}: {key} must be nonempty")
        if not all(isinstance(row[key], str) for key in ("name", "version", "variant")):
            continue
        identity = artifact_id(row)
        if identity in identities:
            errors.append(f"{label}: duplicate artifact identity {identity}")
        identities.add(identity)
        if row.get("status", "not_tested") not in STATUSES:
            errors.append(f"{label}: invalid status")
        if not isinstance(row["scenarios"], list) or not all(
                isinstance(v, str) and v.strip() for v in row["scenarios"]):
            errors.append(f"{label}: scenarios must be a list of names")
        elif not row["scenarios"]:
            errors.append(f"{label}: scenarios must not be empty")
        if not isinstance(row["dependencies"], list):
            errors.append(f"{label}: dependencies must be a list")
        if isinstance(row["dependencies"], list):
            for dependency in row["dependencies"]:
                if not isinstance(dependency, dict) or set(dependency) != {"artifact", "sha256", "required"}:
                    errors.append(f"{label}: dependency needs artifact, sha256 and required")
                    continue
                selected = by_identity.get(dependency["artifact"]) if isinstance(dependency["artifact"], str) else None
                if selected is None:
                    errors.append(f"{label}: missing exact dependency artifact {dependency['artifact']}")
                elif dependency["sha256"] != selected.get("sha256"):
                    errors.append(f"{label}: dependency checksum does not match {dependency['artifact']}")
                if not isinstance(dependency["required"], bool):
                    errors.append(f"{label}: dependency required must be boolean")
        location = row.get("local_path")
        digest = row["sha256"]
        if location is None:
            if digest is not None:
                errors.append(f"{label}: SHA-256 has no local file to verify")
            if row.get("status", "not_tested") != "not_tested":
                errors.append(f"{label}: unavailable artifact must be not_tested")
            continue
        if not isinstance(location, str) or not location:
            errors.append(f"{label}: invalid local_path")
            continue
        path = pathlib.Path(location)
        if not path.is_file():
            errors.append(f"{label}: local file does not exist: {location}")
            continue
        if not isinstance(digest, str) or not SHA256.fullmatch(digest):
            errors.append(f"{label}: invalid SHA-256")
            continue
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual.lower() != digest.lower():
            errors.append(f"{label}: SHA-256 mismatch: {actual}")
    return errors


def _verified_evidence(reference, build, artifact_sha256, scenarios):
    """Read a checksum-pinned comparison artifact; return its observations."""
    if not isinstance(reference, dict) or set(reference) != {"path", "sha256"}:
        return None
    if not isinstance(reference["path"], str) or not isinstance(reference["sha256"], str):
        return None
    if not SHA256.fullmatch(reference["sha256"]):
        return None
    path = pathlib.Path(reference["path"])
    if not path.is_file():
        return None
    try:
        raw = path.read_bytes()
        if hashlib.sha256(raw).hexdigest() != reference["sha256"].lower():
            return None
        payload = json.loads(raw)
    except (OSError, ValueError, UnicodeDecodeError):
        return None
    if not isinstance(payload, dict) or payload.get("schema_version") != 1:
        return None
    if payload.get("build") != build or payload.get("artifact_sha256") != artifact_sha256:
        return None
    axes = payload.get("axes")
    if not isinstance(axes, dict) or any(axes.get(axis) != "pass" for axis in AXES):
        return None
    observations = payload.get("observations")
    if not isinstance(observations, dict) or any(
            not isinstance(observations.get(scenario), str) or
            not observations[scenario].strip() for scenario in scenarios):
        return None
    return {scenario: observations[scenario] for scenario in scenarios}


def _base_passes(row, supplied, axes, target):
    if row.get("local_path") is None or not row["scenarios"]:
        return False
    if not all(value == "pass" for value in axes.values()):
        return False
    if any(not isinstance(row[key], str) or UNKNOWN_METADATA.search(row[key])
           for key in ("source", "license", "api_version", "version")):
        return False
    if not PAPER_BUILD.fullmatch(row["paper_build"]) or not row["paper_build"].startswith(f"paper:{target}:"):
        return False
    foton_build = supplied.get("foton_build")
    if not isinstance(foton_build, str) or not FOTON_BUILD.fullmatch(foton_build):
        return False
    scenario_results = supplied.get("scenarios")
    if not isinstance(scenario_results, dict) or any(
            scenario_results.get(scenario) != "pass" for scenario in row["scenarios"]):
        return False
    if row.get("uses_internals") == "manual_review_required" and supplied.get("reflection_review") != "pass":
        return False
    paper = _verified_evidence(supplied.get("paper_evidence"), row["paper_build"],
                               row["sha256"], row["scenarios"])
    foton = _verified_evidence(supplied.get("foton_evidence"), foton_build,
                               row["sha256"], row["scenarios"])
    return paper is not None and paper == foton


def certification_report(manifest, results):
    errors = validate_manifest(manifest)
    if errors:
        raise ValueError("; ".join(errors))
    rows = []
    products = {}
    base = {}
    for row in manifest["artifacts"]:
        identity = artifact_id(row)
        supplied = results.get(identity, {})
        axes = {axis: supplied.get(axis, "not_tested") for axis in AXES}
        if any(value not in STATUSES for value in axes.values()):
            raise ValueError(f"{identity}: invalid axis status")
        if row.get("local_path") is None and any(value != "not_tested" for value in axes.values()):
            raise ValueError(f"{identity}: unavailable artifact has test results")
        base[identity] = _base_passes(row, supplied, axes, manifest["foton_target"])
        product = (row["name"], row["version"])
        products.setdefault(product, []).append(identity)
        rows.append({"artifact": identity, "name": row["name"], "version": row["version"],
                     "variant": row["variant"], **axes, "certified": False,
                     "paper_evidence": supplied.get("paper_evidence"),
                     "foton_evidence": supplied.get("foton_evidence"),
                     "foton_build": supplied.get("foton_build"),
                     "scenarios": supplied.get("scenarios", {})})
    certified = set()
    # Start from proven leaves; a missing prerequisite or dependency cycle
    # cannot become certified by merely marking each node pass in a JSON file.
    changed = True
    while changed:
        changed = False
        for row in manifest["artifacts"]:
            identity = artifact_id(row)
            if identity in certified or not base[identity]:
                continue
            if all(dependency["artifact"] in certified for dependency in row["dependencies"]
                   if dependency["required"]):
                certified.add(identity)
                changed = True
    for item in rows:
        item["certified"] = item["artifact"] in certified
    passing_products = sum(all(identity in certified for identity in variants)
                           for variants in products.values())
    return {"artifacts": rows, "artifact_count": len(rows),
            "product_count": len(products), "certified_products": passing_products,
            "historical_corpus": manifest.get("historical_corpus", {}),
            "complete_market_corpus": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=pathlib.Path)
    parser.add_argument("--results", type=pathlib.Path,
                        help="JSON mapping artifact identities to five axis statuses")
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    results = json.loads(args.results.read_text(encoding="utf-8")) if args.results else {}
    try:
        report = certification_report(manifest, results)
    except ValueError as error:
        parser.error(str(error))
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
