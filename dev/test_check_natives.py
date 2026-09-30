"""The native audit distinguishes registrations from JNI calls in fixtures."""

import importlib.util
import pathlib
import tempfile
import unittest
from unittest import mock


SPEC = importlib.util.spec_from_file_location(
    "check_natives", pathlib.Path(__file__).with_name("check-natives.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class NativeRegistrationCheck(unittest.TestCase):
    def registrations(self, source):
        with tempfile.TemporaryDirectory() as scratch:
            root = pathlib.Path(scratch)
            (root / "fixture.rs").write_text(source, encoding="utf-8")
            with mock.patch.object(CHECK, "NATIVES_SRC", root):
                return CHECK.registered()

    def test_keeps_multiline_native_registrations(self):
        self.assertEqual(
            self.registrations('''
                method(
                    "entityType",
                    "(Ljava/lang/String;)Ljava/lang/String;",
                    entity_type as *mut c_void,
                )
            '''),
            {("entityType", "(Ljava/lang/String;)Ljava/lang/String;")},
        )

    def test_jni_invocations_are_not_native_registrations(self):
        self.assertEqual(
            self.registrations('''
                env.call_static_method(
                    "SpawnBridgeCheck", "install", "()V", &[],
                );
                env.call_method("receiver", "handle", "()V", &[]);
                method("actualNative", "()V", handler as *mut c_void)
            '''),
            {("actualNative", "()V")},
        )


if __name__ == "__main__":
    unittest.main()
