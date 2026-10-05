"""Regression coverage for the JNI audit's extraction and comparison, not a method whitelist."""

import importlib.util
import pathlib
import re
import sys
import unittest

SPEC = importlib.util.spec_from_file_location(
    "event_bridge", pathlib.Path(__file__).with_name("check-event-bridge.py"))
bridge = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = bridge
SPEC.loader.exec_module(bridge)


class EventBridgeAudit(unittest.TestCase):
    def test_event_literal_keys_have_one_concrete_declaration(self):
        # Narrow guard for relocated concrete events: this is not a Rust type proof.
        root = pathlib.Path(__file__).resolve().parents[1] / "foton-core/src/event"
        declarations = {}
        for path in root.rglob("*.rs"):
            for key in re.findall(r'DowncastTypeKey::new\("(foton:event/[^"\n]+)"\)', path.read_text()):
                declarations.setdefault(key, []).append(str(path.relative_to(root)))
        self.assertTrue(declarations, "event key inventory must not silently become empty")
        self.assertEqual({key: paths for key, paths in declarations.items() if len(paths) > 1}, {})

    def source(self, descriptor="(Ljava/lang/String;DDD)Z", values=None):
        if values is None:
            values = "JValue::Object(&world), JValue::Double(x), JValue::Double(y), JValue::Double(z)"
        return '''
            const BRIDGE: &str = "foton/EventBridge";
            fn fire(env: JNIEnv) {
                env.call_static_method(BRIDGE, "fireSpawn", "''' + descriptor + '''", &[''' + values + ''']);
            }
        '''

    def test_descriptor_typo_and_argument_mismatch_are_both_reported(self):
        declared = {"foton.EventBridge": {("fireSpawn", "(Ljava/lang/String;DDD)Z")}}
        self.assertEqual(bridge.check(bridge.calls(self.source()), declared), [])
        errors = bridge.check(bridge.calls(self.source("(Ljava/lang/String;DDDD)Z")), declared)
        self.assertEqual(len(errors), 2)
        self.assertIn("not declared static", errors[0])
        self.assertIn("but Rust passes", errors[1])

    def test_correct_descriptor_does_not_hide_wrong_argument_type(self):
        declared = {"foton.EventBridge": {("fireSpawn", "(Ljava/lang/String;DDD)Z")}}
        source = self.source().replace("JValue::Double(y)", "JValue::Int(y)")
        errors = bridge.check(bridge.calls(source), declared)
        self.assertEqual(len(errors), 1)
        self.assertIn("but Rust passes", errors[0])

    def test_helper_resolves_every_caller_and_nested_arguments(self):
        source = '''
            const BRIDGE: &str = "foton/EventBridge";
            fn run() {
                event(&vm, "first", nested(a, (b, c)));
                event(&vm, "second", anything());
            }
            fn event(vm: &JavaVM, method: &str, payload: &str) {
                env.call_static_method(BRIDGE, method, "([B)Z", &[JValue::Object(&bytes)]);
            }
        '''
        found = bridge.calls(source)
        self.assertEqual({call.name for call in found}, {"first", "second"})
        errors = bridge.check(found, {"foton.EventBridge": {("first", "([B)Z")}})
        self.assertEqual(len(errors), 1)
        self.assertIn(".second([B)Z", errors[0])
        for changed in (
            source.replace('event(&vm, "second", anything());', 'let alias = event; alias(&vm, "second", anything());'),
            source.replace('env.call_static_method', 'let method = "rewritten"; env.call_static_method'),
        ):
            with self.subTest(source=changed), self.assertRaises(ValueError):
                bridge.calls(changed)

    def test_unresolvable_call_is_an_error_instead_of_a_skipped_event(self):
        source = self.source()
        for changed in (
            source.replace('"fireSpawn"', 'choose_method()'),
            source.replace('"(Ljava/lang/String;DDD)Z"', 'signature'),
            source.replace("&[JValue::Object(&world), JValue::Double(x), JValue::Double(y), JValue::Double(z)]", "&args"),
        ):
            with self.subTest(source=changed), self.assertRaises(ValueError):
                bridge.calls(changed)

    def test_comments_do_not_create_calls_and_empty_audit_fails(self):
        comments = '''
            // env.call_static_method(NOPE, "missing", "()V", &[]);
            /* outer /* nested */ env.call_static_method(NOPE, "missing", "()V", &[]); */
        '''
        self.assertEqual(len(bridge.calls(comments + self.source())), 1)
        with self.assertRaisesRegex(ValueError, "no JNI calls"):
            bridge.calls(comments)

    def test_rust_strings_and_characters_cannot_hide_real_calls(self):
        source = r'''let quote = '"'; let literal = r##"embedded " // not a comment"##;'''
        self.assertEqual(len(bridge.calls(source + self.source())), 1)
        with self.assertRaisesRegex(ValueError, "no JNI calls"):
            bridge.calls(r'''let literal = r#"env.call_static_method(BRIDGE, "fake", "()V", &[]);"#;''')

    def test_javap_overloads_are_preserved_and_instance_methods_excluded(self):
        output = '''
          public static boolean fire(java.lang.String);
            descriptor: (Ljava/lang/String;)Z
          public static boolean fire(java.lang.String, int);
            descriptor: (Ljava/lang/String;I)Z
          public boolean instance();
            descriptor: ()Z
          private static final java.lang.String FIELD;
            descriptor: Ljava/lang/String;
        '''
        self.assertEqual(bridge.declarations(output), {
            ("fire", "(Ljava/lang/String;)Z"), ("fire", "(Ljava/lang/String;I)Z")})


if __name__ == "__main__":
    unittest.main()
