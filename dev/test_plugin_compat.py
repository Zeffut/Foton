"""Corpus and certification report invariants."""
import hashlib
import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import plugin_compat


class Manifest(unittest.TestCase):
    def manifest(self, *rows):
        return {"foton_target": "26.2", "artifacts": list(rows)}

    def row(self, path, **changes):
        row = dict(name="Example", version="1.0", variant="release",
                   sha256=hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None,
                   local_path=str(path) if path.exists() else None,
                   source="local build", license="proprietary",
                   api_version="1.21", paper_build="paper:26.2:123",
                   dependencies=[], scenarios=["startup"])
        row.update(changes)
        return row

    def passing(self, directory, row):
        result = {axis: "pass" for axis in plugin_compat.AXES}
        result.update(foton_build="foton:" + "a" * 40,
                      scenarios={scenario: "pass" for scenario in row["scenarios"]})
        for kind, build in (("paper", row["paper_build"]), ("foton", result["foton_build"])):
            path = directory / f"{row['name']}-{kind}.json"
            payload = dict(schema_version=1, artifact_sha256=row["sha256"], build=build,
                           axes={axis: "pass" for axis in plugin_compat.AXES},
                           observations={scenario: "same" for scenario in row["scenarios"]})
            path.write_text(json.dumps(payload), encoding="utf-8")
            result[f"{kind}_evidence"] = {
                "path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        return result

    def test_hash_identity_dependency_and_metadata_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            self.assertEqual(plugin_compat.validate_manifest(self.manifest(row)), [])
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(row, row)))
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(self.row(path, sha256="0" * 64))))
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(self.row(path, dependencies=[{"artifact": "missing@1#release", "sha256": "0" * 64, "required": True}]))))
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(self.row(path, source="", license=""))))

    def test_variants_share_product_but_dependencies_select_exact_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "fixture.jar"
            path.write_bytes(b"jar")
            release = self.row(path)
            shaded = self.row(path, variant="shaded")
            self.assertEqual(plugin_compat.validate_manifest(self.manifest(release, shaded)), [])
            self.assertEqual(plugin_compat.product_count([release, shaded]), 1)
            provider_a = self.row(path, name="Provider", version="1")
            provider_b = self.row(path, name="Provider", version="2")
            ambiguous = self.row(path, dependencies=[{"artifact": "Provider", "sha256": provider_a["sha256"], "required": True}])
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(ambiguous, provider_a, provider_b)))
            pinned = self.row(path, dependencies=[{"artifact": plugin_compat.artifact_id(provider_b), "sha256": provider_b["sha256"], "required": True}])
            self.assertEqual(plugin_compat.validate_manifest(self.manifest(pinned, provider_a, provider_b)), [])
            wrong_hash = self.row(path, dependencies=[{"artifact": plugin_compat.artifact_id(provider_b), "sha256": "0" * 64, "required": True}])
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(wrong_hash, provider_a, provider_b)))

    def test_unavailable_artifact_stays_not_tested(self):
        row = self.row(pathlib.Path("absent.jar"))
        self.assertEqual(plugin_compat.validate_manifest(self.manifest(row)), [])
        result = plugin_compat.certification_report(self.manifest(row), {})
        self.assertEqual(result["artifacts"][0]["linkage"], "not_tested")
        self.assertEqual(result["certified_products"], 0)

    def test_required_unavailable_dependency_blocks_observer(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            path = root / "observer.jar"
            path.write_bytes(b"jar")
            packet = self.row(root / "absent.jar", name="packetevents", version="2.13.0", variant="spigot")
            observer = self.row(path, name="Observer", dependencies=[{
                "artifact": plugin_compat.artifact_id(packet), "sha256": None, "required": True}])
            result = plugin_compat.certification_report(
                self.manifest(observer, packet),
                {plugin_compat.artifact_id(observer): self.passing(root, observer)})
            self.assertFalse(result["artifacts"][0]["certified"])
            observer["dependencies"][0]["required"] = False
            result = plugin_compat.certification_report(
                self.manifest(observer, packet),
                {plugin_compat.artifact_id(observer): self.passing(root, observer)})
            self.assertTrue(result["artifacts"][0]["certified"])

    def test_all_axes_scenarios_and_evidence_are_required(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            path = root / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            key = plugin_compat.artifact_id(row)
            passing = self.passing(root, row)
            self.assertEqual(plugin_compat.certification_report(self.manifest(row), {key: passing})["certified_products"], 1)
            passing["behavior"] = "not_tested"
            self.assertEqual(plugin_compat.certification_report(self.manifest(row), {key: passing})["certified_products"], 0)
            passing["behavior"] = "pass"
            passing["paper_evidence"]["path"] = str(root / "absent.json")
            self.assertEqual(plugin_compat.certification_report(self.manifest(row), {key: passing})["certified_products"], 0)
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(self.row(path, scenarios=[]))))

    def test_placeholders_and_mismatched_oracles_cannot_certify(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            path = root / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            key = plugin_compat.artifact_id(row)
            passing = self.passing(root, row)
            for field, value in (("paper_build", "not_verified"), ("source", "not_verified"),
                                 ("license", "not_verified"), ("api_version", "not_verified")):
                changed = dict(row, **{field: value})
                self.assertEqual(plugin_compat.certification_report(self.manifest(changed), {key: passing})["certified_products"], 0)
            wrong_target = dict(row, paper_build="paper:1.21.11:123")
            self.assertEqual(plugin_compat.certification_report(self.manifest(wrong_target), {key: passing})["certified_products"], 0)
            passing["foton_build"] = "made-up-build"
            self.assertEqual(plugin_compat.certification_report(self.manifest(row), {key: passing})["certified_products"], 0)
            passing["foton_build"] = "foton:" + "a" * 40
            paper_path = pathlib.Path(passing["paper_evidence"]["path"])
            payload = json.loads(paper_path.read_text(encoding="utf-8"))
            payload["observations"]["startup"] = "different"
            paper_path.write_text(json.dumps(payload), encoding="utf-8")
            passing["paper_evidence"]["sha256"] = hashlib.sha256(paper_path.read_bytes()).hexdigest()
            self.assertEqual(plugin_compat.certification_report(self.manifest(row), {key: passing})["certified_products"], 0)

    def test_report_status_is_derived_and_manifest_status_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            path = root / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            row["status"] = "not_tested"
            self.assertTrue(plugin_compat.validate_manifest(self.manifest(row)))
            row.pop("status")
            key = plugin_compat.artifact_id(row)
            untested = plugin_compat.certification_report(self.manifest(row), {})
            self.assertEqual(untested["artifacts"][0]["status"], "not_tested")
            tested = plugin_compat.certification_report(
                self.manifest(row), {key: self.passing(root, row)})
            self.assertEqual(tested["artifacts"][0]["status"], "pass")
            self.assertTrue(tested["artifacts"][0]["certified"])


if __name__ == "__main__":
    unittest.main()
