"""Focused regressions for shared server-log diagnostics."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from dev.compat.compare import find_unexpected_errors as comparator_errors
from dev.compat.log_diagnostics import find_unexpected_errors


SCRIPT = Path(__file__).resolve().parent / "compat" / "log_diagnostics.py"


class LogDiagnosticsTests(unittest.TestCase):
    def test_java_exception_forms(self):
        lines = [
            "Exception: plugin invocation failed",
            "Caused by: Exception: plugin invocation failed",
            "java.lang.ClassNotFoundException: foton.entity.CraftPlayer",
            "Caused by: org.example.Outer$NestedException: failed",
            'Exception in thread "main" java.lang.IllegalStateException: bad',
            "java.lang.LinkageError: incompatible class",
        ]
        self.assertEqual(lines, find_unexpected_errors(lines))

    def test_retained_via_failure_excerpt(self):
        lines = [
            "[host] enabled ViaVersion v5.11.0",
            "java.lang.ClassNotFoundException: foton.entity.CraftPlayer",
            "[host] enabled ViaBackwards v5.11.0",
        ]
        self.assertEqual([lines[1]], comparator_errors(lines))

    def test_host_plugin_codec_and_reference_count(self):
        lines = [
            "[host] ViaVersion failed during enable",
            "[host] failed to enable ViaBackwards",
            "Plugin failed to load",
            "could not enable plugin TestPlugin",
            "DecoderException: malformed packet",
            "EncoderException: malformed packet",
            "ReferenceCountUtil released twice",
            "refCnt: 0",
            "refcount failure",
            "JNI invocation failed",
        ]
        self.assertEqual(lines, find_unexpected_errors(lines))

    def test_warnings_do_not_exempt_adjacent_exception(self):
        warnings = [
            "Failed to load Minecraft services public keys: offline",
            "WARNING: Could not check for updates, check your connection.",
            "[INFO] Recovery from previous Error-free startup completed",
        ]
        self.assertEqual([], find_unexpected_errors(warnings))
        self.assertEqual(["java.net.UnknownHostException: host"],
                         find_unexpected_errors(warnings + ["java.net.UnknownHostException: host"]))

    def test_cli_reports_line_numbers_and_nonzero(self):
        with tempfile.TemporaryDirectory() as scratch:
            log = Path(scratch) / "server.log"
            log.write_text("ready\njava.lang.ClassNotFoundException: CraftPlayer\n", encoding="utf-8")
            result = subprocess.run([sys.executable, str(SCRIPT), str(log)],
                                    capture_output=True, text=True, cwd=scratch)
            self.assertNotEqual(0, result.returncode)
            self.assertIn("2:java.lang.ClassNotFoundException", result.stderr)
            missing = subprocess.run([sys.executable, str(SCRIPT), str(log) + ".missing"],
                                     capture_output=True, text=True, cwd=scratch)
            self.assertNotEqual(0, missing.returncode)
            log.write_text("Error-free startup\n", encoding="utf-8")
            clean = subprocess.run([sys.executable, str(SCRIPT), str(log)],
                                   capture_output=True, text=True, cwd=scratch)
            self.assertEqual(0, clean.returncode)


if __name__ == "__main__":
    unittest.main()
