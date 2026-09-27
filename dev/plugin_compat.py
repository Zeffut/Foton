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
REQUIRED = ("name", "version", "variant", "sha256", "source", "license",
            "api_version", "paper_build", "dependencies", "scenarios")


def artifact_id(row):
    return f"{row['name']}@{row['version']}#{row['variant']}"


def product_count(rows):
    return len({(row["name"], row["version"]) for row in rows})


def validate_manifest(manifest):
    errors = []
    rows = manifest.get("artifacts")
    if not isinstance(rows, list):
        return ["artifacts must be a list"]
    identities = set()
    names = {row.get("name") for row in rows if isinstance(row, dict)}
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
        for key in ("dependencies", "scenarios"):
            if not isinstance(row[key], list) or not all(isinstance(v, str) and v for v in row[key]):
                errors.append(f"{label}: {key} must be a list of names")
        if isinstance(row["dependencies"], list):
            for dependency in row["dependencies"]:
                if isinstance(dependency, str) and dependency not in names:
                    errors.append(f"{label}: missing dependency row {dependency}")
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
        if not isinstance(digest, str) or not re.fullmatch(r"[a-fA-F0-9]{64}", digest):
            errors.append(f"{label}: invalid SHA-256")
            continue
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual.lower() != digest.lower():
            errors.append(f"{label}: SHA-256 mismatch: {actual}")
    return errors


def certification_report(manifest, results):
    errors = validate_manifest(manifest)
    if errors:
        raise ValueError("; ".join(errors))
    rows = []
    passing_products = set()
    products = {}
    for row in manifest["artifacts"]:
        identity = artifact_id(row)
        supplied = results.get(identity, {})
        axes = {axis: supplied.get(axis, "not_tested") for axis in AXES}
        if any(value not in STATUSES for value in axes.values()):
            raise ValueError(f"{identity}: invalid axis status")
        if row.get("local_path") is None and any(value != "not_tested" for value in axes.values()):
            raise ValueError(f"{identity}: unavailable artifact has test results")
        scenario_results = supplied.get("scenarios", {})
        scenario_pass = isinstance(scenario_results, dict) and all(
            scenario_results.get(scenario) == "pass" for scenario in row["scenarios"])
        evidence = supplied.get("evidence")
        foton_build = supplied.get("foton_build")
        oracle_pinned = not row["paper_build"].startswith("not_recorded")
        reflection_reviewed = (row.get("uses_internals") != "manual_review_required"
                               or supplied.get("reflection_review") == "pass")
        passed = (all(value == "pass" for value in axes.values()) and scenario_pass
                  and isinstance(evidence, str) and bool(evidence.strip())
                  and isinstance(foton_build, str) and bool(foton_build.strip())
                  and oracle_pinned and reflection_reviewed)
        product = (row["name"], row["version"])
        products.setdefault(product, []).append(passed)
        rows.append({"artifact": identity, "name": row["name"], "version": row["version"],
                     "variant": row["variant"], **axes, "certified": passed,
                     "evidence": evidence, "foton_build": foton_build,
                     "scenarios": scenario_results})
    for product, variants in products.items():
        if all(variants):
            passing_products.add(product)
    return {"artifacts": rows, "artifact_count": len(rows),
            "product_count": len(products), "certified_products": len(passing_products),
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
