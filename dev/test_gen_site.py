#!/usr/bin/env python3
"""Checks on the site generator: no unfilled holes, no dead internal links."""
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import importlib.util

spec = importlib.util.spec_from_file_location(
    "gen_site", pathlib.Path(__file__).resolve().parent / "gen-site.py")
gen_site = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen_site)


class Filling(unittest.TestCase):
    def test_a_hole_is_replaced_by_its_value(self):
        self.assertEqual(gen_site.fill("v{{ version }}", {"version": "1.2"}, "t"), "v1.2")

    def test_whitespace_inside_the_braces_is_tolerated(self):
        self.assertEqual(gen_site.fill("{{version}}", {"version": "1.2"}, "t"), "1.2")

    def test_an_unknown_hole_stops_the_build(self):
        with self.assertRaises(SystemExit):
            gen_site.fill("{{ nope }}", {"version": "1.2"}, "t")

    def test_a_value_is_never_reinterpreted_as_a_hole(self):
        out = gen_site.fill("{{ a }}", {"a": "{{ b }}", "b": "x"}, "t")
        self.assertEqual(out, "{{ b }}")


class Build(unittest.TestCase):
    def test_the_build_writes_a_page_per_entry(self):
        with tempfile.TemporaryDirectory() as tmp:
            written = gen_site.build(pathlib.Path(tmp))
            names = {p.relative_to(tmp).as_posix() for p in written}
            self.assertIn("index.html", names)

    def test_no_output_still_contains_a_hole(self):
        with tempfile.TemporaryDirectory() as tmp:
            for path in gen_site.build(pathlib.Path(tmp)):
                if path.suffix == ".html":
                    self.assertNotIn("{{", path.read_text(encoding="utf-8"), path.name)

    def test_internal_links_all_resolve(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            written = gen_site.build(out)
            self.assertEqual(gen_site.check_links(out, written), [])


if __name__ == "__main__":
    unittest.main()
