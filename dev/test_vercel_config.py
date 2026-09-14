import fnmatch
import json
import unittest
from pathlib import Path


def expand_brace_glob(pattern):
    if not (pattern.startswith("{") and pattern.endswith("}")):
        return [pattern]
    return pattern[1:-1].split(",")


class VercelFunctionBundleConfigTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        config_path = Path(__file__).parents[1] / "vercel.json"
        cls.config = json.loads(config_path.read_text())
        cls.exclusions = cls.config["functions"]["api/**/*.py"]["excludeFiles"]
        cls.patterns = expand_brace_glob(cls.exclusions)

    def is_excluded(self, path):
        return any(fnmatch.fnmatch(path, pattern) for pattern in self.patterns)

    def test_api_report_is_kept_in_the_function_bundle(self):
        self.assertFalse(self.is_excluded("api/report.py"))

    def test_heavyweight_trees_are_excluded(self):
        excluded_paths = [
            ".cargo/config.toml",
            ".github/workflows/ci.yml",
            ".superpowers/sdd/brief.md",
            "design/mockup.html",
            "dev/gen-site.py",
            "docs/index.md",
            "foton/src/lib.rs",
            "foton-core/src/lib.rs",
            "package-content/assets/logo.svg",
            "plugin-api/src/lib.rs",
            "plugins/example/plugin.json",
            "site/dist/index.html",
        ]
        for path in excluded_paths:
            with self.subTest(path=path):
                self.assertTrue(self.is_excluded(path))


if __name__ == "__main__":
    unittest.main()
