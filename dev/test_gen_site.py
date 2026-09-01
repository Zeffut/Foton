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
    def test_configuration_reference_covers_every_shipped_schema(self):
        reference = gen_site.configuration_reference_html(gen_site.strings("en"))
        for toml_name, _, _, _ in gen_site.cs.FILES:
            self.assertIn(f"<code>{toml_name}</code>", reference)

    def test_the_build_writes_a_page_per_entry(self):
        with tempfile.TemporaryDirectory() as tmp:
            written = gen_site.build(pathlib.Path(tmp))
            names = {p.relative_to(tmp).as_posix() for p in written}
            self.assertIn("en/index.html", names)

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


class Bilingual(unittest.TestCase):
    def test_both_editions_are_built(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            written = {p.relative_to(out).as_posix() for p in gen_site.build(out)}
            self.assertIn("en/index.html", written)
            self.assertIn("fr/index.html", written)

    def test_the_french_page_declares_french(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            gen_site.build(out)
            self.assertIn('lang="fr"', (out / "fr/index.html").read_text(encoding="utf-8"))
            self.assertIn('lang="en"', (out / "en/index.html").read_text(encoding="utf-8"))

    def test_french_nav_links_stay_inside_french(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            gen_site.build(out)
            page = (out / "fr/index.html").read_text(encoding="utf-8")
            nav = page.split('<nav>')[1].split('</nav>')[0]
            self.assertTrue(nav.count('href="/fr/') >= 1)
            self.assertNotIn('href="/start/"', nav)

    def test_each_edition_points_at_the_other(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            gen_site.build(out)
            self.assertIn('href="/fr/"', (out / "en/index.html").read_text(encoding="utf-8"))
            self.assertIn('href="/en/"', (out / "fr/index.html").read_text(encoding="utf-8"))

    def test_the_switcher_stays_on_the_same_page(self):
        """Switching language from /fr/start/ lands on /en/start/, not on the
        English home -- losing someone's place is the commonest i18n defect."""
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            gen_site.build(out)
            page = (out / "fr/start/index.html").read_text(encoding="utf-8")
            self.assertIn('href="/en/start/"', page)

    def test_a_missing_translation_stops_the_build(self):
        with self.assertRaises(SystemExit):
            gen_site.strings("fr")["no_such_key"]

    def test_links_resolve_in_both_editions(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = pathlib.Path(tmp)
            self.assertEqual(gen_site.check_links(out, gen_site.build(out)), [])


class PlatformPaths(unittest.TestCase):
    """The /_vercel/ exemption must let the host's own paths through without
    blunting the check that catches a genuinely dead link."""

    def _page(self, tmp, href):
        out = pathlib.Path(tmp)
        written = gen_site.build(out)
        page = out / "en" / "index.html"
        page.write_text(f'<a href="{href}">x</a>', encoding="utf-8")
        return out, written

    def test_a_platform_path_is_not_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, written = self._page(tmp, "/_vercel/insights/script.js")
            self.assertEqual(gen_site.check_links(out, written), [])

    def test_a_dead_link_is_still_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, written = self._page(tmp, "/nope/")
            self.assertEqual(len(gen_site.check_links(out, written)), 1)

    def test_a_lookalike_prefix_is_not_exempt(self):
        with tempfile.TemporaryDirectory() as tmp:
            out, written = self._page(tmp, "/_vercelish/x.js")
            self.assertEqual(len(gen_site.check_links(out, written)), 1)


if __name__ == "__main__":
    unittest.main()
