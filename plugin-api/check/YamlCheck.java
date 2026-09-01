import java.util.List;
import java.util.Map;

/** The YAML reader, on the shapes plugin configuration actually contains. */
final class YamlCheck {
    private YamlCheck() {}

    @SuppressWarnings("unchecked")
    static void check() {
        Map<String, Object> root = (Map<String, Object>) foton.Yaml.load(String.join("\n",
            "# a leading comment",
            "name: Example",
            "port: 25565",
            "ratio: 0.5",
            "enabled: yes",
            "disabled: off",
            "missing: ~",
            "quoted: '42'",
            "url: http://example.invalid/path",
            "at: 12:30",
            "hash: 'a # b'",
            "trailing: value # not part of it",
            "nested:",
            "  deep:",
            "    key: found",
            "depend:",
            "- Vault",
            "- Essentials",
            "indented:",
            "  - one",
            "  - two",
            "flow: [a, b, 3]",
            "links:",
            "- http://example.invalid/one",
            "- 12:30",
            "people:",
            "  - name: Ada",
            "    age: 36",
            "  - name: Alan",
            "    age: 41",
            "message: |",
            "  first",
            "  second",
            "folded: >",
            "  wrapped",
            "  together",
            ""));

        Checks.same(root.get("name"), "Example", "a plain string");
        Checks.same(root.get("port"), 25565, "an int stays an int");
        Checks.same(root.get("ratio"), 0.5, "a double stays a double");
        // YAML 1.1, which is what SnakeYAML resolves and what Paper therefore
        // reads. A config saying `enabled: yes` means true.
        Checks.same(root.get("enabled"), Boolean.TRUE, "yes is a boolean");
        Checks.same(root.get("disabled"), Boolean.FALSE, "off is a boolean");
        Checks.expect(root.containsKey("missing") && root.get("missing") == null,
            "~ is a present null");
        Checks.same(root.get("quoted"), "42", "a quoted number stays a string");

        // A colon inside a value is not a key boundary; a URL and a clock time
        // are the two that break naive splitting.
        Checks.same(root.get("url"), "http://example.invalid/path", "a URL keeps its colon");
        Checks.same(root.get("at"), "12:30", "a time keeps its colon");
        Checks.same(root.get("hash"), "a # b", "a quoted hash is not a comment");
        Checks.same(root.get("trailing"), "value", "a trailing comment is dropped");

        Map<String, Object> nested = (Map<String, Object>) root.get("nested");
        Checks.same(((Map<String, Object>) nested.get("deep")).get("key"), "found",
            "two levels of nesting");

        // A sequence at its key's own indent, which is how plugin.yml writes
        // `depend:`, and the indented form, which is how config.yml does.
        Checks.same(root.get("depend"), List.of("Vault", "Essentials"), "an unindented sequence");
        Checks.same(root.get("indented"), List.of("one", "two"), "an indented sequence");
        Checks.same(root.get("flow"), List.of("a", "b", 3), "a flow sequence");

        // In a sequence, a colon is the only thing separating an entry that is
        // a value from an entry that is a mapping. A colon not followed by a
        // space is part of the value: a URL and a clock time are the two that
        // a naive split turns into single-key maps.
        Checks.same(root.get("links"), List.of("http://example.invalid/one", "12:30"),
            "a sequence entry containing a colon is a value, not a mapping");

        List<Map<String, Object>> people = (List<Map<String, Object>>) root.get("people");
        Checks.same(people.size(), 2, "a sequence of mappings");
        Checks.same(people.get(0).get("name"), "Ada", "the key sharing the dash's line");
        Checks.same(people.get(0).get("age"), 36, "the key below it");
        Checks.same(people.get(1).get("name"), "Alan", "the second entry");

        Checks.same(root.get("message"), "first\nsecond\n", "a literal block keeps its breaks");
        Checks.same(root.get("folded"), "wrapped together\n", "a folded block joins its lines");

        roundTrip();
    }

    /** What is written has to read back as what it was. */
    @SuppressWarnings("unchecked")
    private static void roundTrip() {
        java.util.Map<String, Object> original = new java.util.LinkedHashMap<>();
        original.put("name", "Example");
        original.put("port", 25565);
        original.put("enabled", true);
        // Strings that would come back as something else unless quoted.
        original.put("version", "1.0");
        original.put("answer", "yes");
        original.put("empty", "");
        original.put("spaced", " padded ");
        original.put("colon", "key: value");
        original.put("list", List.of("a", "b"));
        java.util.Map<String, Object> inner = new java.util.LinkedHashMap<>();
        inner.put("deep", "value");
        original.put("nested", inner);

        String written = foton.Yaml.dump(original);
        Map<String, Object> read = (Map<String, Object>) foton.Yaml.load(written);
        for (Map.Entry<String, Object> entry : original.entrySet()) {
            Checks.same(read.get(entry.getKey()), entry.getValue(),
                "round trip of " + entry.getKey() + " through:\n" + written);
        }
    }
}
