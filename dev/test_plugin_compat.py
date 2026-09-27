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
    def row(self, path, **changes):
        row = dict(name="Example", version="1.0", variant="release",
                   sha256=hashlib.sha256(path.read_bytes()).hexdigest() if path.exists() else None,
                   local_path=str(path), source="local build", license="proprietary",
                   api_version="1.21", paper_build="Paper-test-build",
                   dependencies=[], scenarios=["startup"], status="not_tested")
        row.update(changes)
        return row

    def test_hash_identity_dependency_and_metadata_validation(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            manifest = {"artifacts": [row]}
            self.assertEqual(plugin_compat.validate_manifest(manifest), [])
            self.assertTrue(plugin_compat.validate_manifest({"artifacts": [row, row]}))
            self.assertTrue(plugin_compat.validate_manifest({"artifacts": [self.row(path, sha256="0" * 64)]}))
            self.assertTrue(plugin_compat.validate_manifest({"artifacts": [self.row(path, dependencies=["missing"])]}))
            self.assertTrue(plugin_compat.validate_manifest({"artifacts": [self.row(path, source="", license="")]}))

    def test_variants_share_product_but_have_separate_artifact_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "fixture.jar"
            path.write_bytes(b"jar")
            rows = [self.row(path), self.row(path, variant="shaded")]
            self.assertEqual(plugin_compat.validate_manifest({"artifacts": rows}), [])
            self.assertEqual(plugin_compat.product_count(rows), 1)

    def test_unavailable_artifact_stays_not_tested(self):
        row = self.row(pathlib.Path("absent.jar"), sha256=None, local_path=None)
        self.assertEqual(plugin_compat.validate_manifest({"artifacts": [row]}), [])
        result = plugin_compat.certification_report({"artifacts": [row]}, {})
        self.assertEqual(result["artifacts"][0]["linkage"], "not_tested")
        self.assertEqual(result["certified_products"], 0)

    def test_all_five_axes_are_required_for_certification(self):
        with tempfile.TemporaryDirectory() as directory:
            path = pathlib.Path(directory) / "fixture.jar"
            path.write_bytes(b"jar")
            row = self.row(path)
            key = plugin_compat.artifact_id(row)
            passing = {axis: "pass" for axis in plugin_compat.AXES}
            passing.update(evidence="logs/test.json", foton_build="foton-test-build",
                           scenarios={"startup": "pass"})
            report = plugin_compat.certification_report({"artifacts": [row]}, {key: passing})
            self.assertEqual(report["certified_products"], 1)
            passing["behavior"] = "not_tested"
            report = plugin_compat.certification_report({"artifacts": [row]}, {key: passing})
            self.assertEqual(report["certified_products"], 0)
            passing["behavior"] = "pass"
            del passing["evidence"]
            report = plugin_compat.certification_report({"artifacts": [row]}, {key: passing})
            self.assertEqual(report["certified_products"], 0)


if __name__ == "__main__":
    unittest.main()
