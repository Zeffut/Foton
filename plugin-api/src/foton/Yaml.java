package foton;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/** Reads and writes the YAML that plugin configuration is written in.
 *
 * Written here rather than taken from SnakeYAML because the API jar has to
 * compile from a fresh clone with nothing downloaded, and because a config
 * file is data: this reader cannot name a Java class, so reading a file can
 * never become running code. SnakeYAML stays available at runtime in
 * plugin-api/lib for plugins that import it themselves.
 *
 * The subset is the one configuration files use: block mappings, block and
 * flow sequences, flow mappings, quoted and plain scalars, literal and folded
 * block scalars, and comments. Anchors, aliases, tags, multiple documents and
 * complex keys are not read -- no plugin config in the surveyed corpus uses
 * them, and guessing at them would be worse than saying so.
 *
 * Scalar types follow YAML 1.1, which is what SnakeYAML resolves: `yes`, `no`,
 * `on` and `off` are booleans. That surprises people, but a config that Paper
 * reads as a boolean has to read as a boolean here too.
 */
public final class Yaml {
    private Yaml() {}

    /** Reads a document. Answers a Map, a List, a scalar, or null. */
    public static Object load(String text) {
        Reader reader = new Reader(text);
        return reader.document();
    }

    /** Writes a document in the block style plugins expect to read back. */
    public static String dump(Object value) {
        StringBuilder out = new StringBuilder();
        writeNode(out, value, 0, false);
        return out.toString();
    }

    // read

    /** One line, kept with its indent so structure is a column comparison. */
    private static final class Line {
        String raw;
        int indent;
        String content;

        Line(String raw) {
            set(raw);
        }

        void set(String raw) {
            this.raw = raw;
            int at = 0;
            while (at < raw.length() && raw.charAt(at) == ' ') {
                at++;
            }
            this.indent = at;
            this.content = raw.substring(at).stripTrailing();
        }

        boolean skippable() {
            return content.isEmpty() || content.charAt(0) == '#';
        }
    }

    private static final class Reader {
        private final List<Line> lines = new ArrayList<>();
        private int at;

        Reader(String text) {
            for (String raw : text.replace("\r\n", "\n").replace('\r', '\n').split("\n", -1)) {
                lines.add(new Line(expandTabs(raw)));
            }
        }

        /** Tabs are not valid YAML indentation; treating them as spaces reads
         * a file a human wrote by hand rather than refusing it. */
        private static String expandTabs(String raw) {
            if (raw.indexOf('\t') < 0) {
                return raw;
            }
            StringBuilder out = new StringBuilder();
            for (int i = 0; i < raw.length(); i++) {
                char c = raw.charAt(i);
                if (c == '\t' && out.toString().isBlank()) {
                    out.append("  ");
                } else {
                    out.append(c);
                }
            }
            return out.toString();
        }

        Object document() {
            skipIgnorable();
            if (at >= lines.size()) {
                return null;
            }
            // A leading "---" opens a document; there is only ever one here.
            if (lines.get(at).content.equals("---")) {
                at++;
                skipIgnorable();
            }
            if (at >= lines.size()) {
                return null;
            }
            return node(lines.get(at).indent);
        }

        private void skipIgnorable() {
            while (at < lines.size() && lines.get(at).skippable()) {
                at++;
            }
        }

        /** Whatever sits at this indent: a sequence, a mapping, or nothing. */
        private Object node(int indent) {
            skipIgnorable();
            if (at >= lines.size() || lines.get(at).indent < indent) {
                return null;
            }
            return isDash(lines.get(at).content) ? sequence(indent) : mapping(indent);
        }

        private static boolean isDash(String content) {
            return content.equals("-") || content.startsWith("- ");
        }

        private Map<String, Object> mapping(int indent) {
            Map<String, Object> out = new LinkedHashMap<>();
            while (true) {
                skipIgnorable();
                if (at >= lines.size()) {
                    return out;
                }
                Line line = lines.get(at);
                if (line.indent < indent || isDash(line.content)) {
                    return out;
                }
                int colon = keyEnd(line.content);
                if (colon < 0) {
                    // Not a mapping entry. Rather than guess, stop: whatever
                    // follows belongs to whoever called.
                    return out;
                }
                String key = unquote(line.content.substring(0, colon).strip());
                String rest = stripComment(line.content.substring(colon + 1).strip());
                at++;
                out.put(key, rest.isEmpty() ? child(indent) : inline(rest, indent));
            }
        }

        /** The value written under a key rather than beside it.
         *
         * A sequence may sit at the key's own indent -- `depend:` followed by
         * unindented dashes is how half of plugin.yml is written -- so that
         * case is accepted at an equal column, while a mapping must be deeper
         * or it would be the key's own sibling.
         */
        private Object child(int indent) {
            skipIgnorable();
            if (at >= lines.size()) {
                return null;
            }
            Line line = lines.get(at);
            if (line.indent > indent) {
                return node(line.indent);
            }
            if (line.indent == indent && isDash(line.content)) {
                return sequence(indent);
            }
            return null;
        }

        /** A value on the same line as its key, or a block scalar header. */
        private Object inline(String rest, int indent) {
            char first = rest.charAt(0);
            if (first == '|' || first == '>') {
                return blockScalar(rest, indent);
            }
            return scalar(rest);
        }

        private List<Object> sequence(int indent) {
            List<Object> out = new ArrayList<>();
            while (true) {
                skipIgnorable();
                if (at >= lines.size()) {
                    return out;
                }
                Line line = lines.get(at);
                if (line.indent != indent || !isDash(line.content)) {
                    return out;
                }
                String rest = line.content.equals("-")
                    ? ""
                    : stripComment(line.content.substring(2).strip());
                if (rest.isEmpty()) {
                    at++;
                    out.add(child(indent));
                    continue;
                }
                if (keyEnd(rest) >= 0) {
                    // `- key: value` is a mapping whose first key happens to
                    // share the dash's line. Blanking the dash turns it into
                    // an ordinary mapping line, and the rest reads itself.
                    int column = line.raw.indexOf('-', line.indent) + 1;
                    while (column < line.raw.length() && line.raw.charAt(column) == ' ') {
                        column++;
                    }
                    line.set(" ".repeat(column) + line.raw.substring(column));
                    out.add(mapping(column));
                    continue;
                }
                at++;
                out.add(inline(rest, indent));
            }
        }

        /** A literal (`|`) or folded (`>`) block, with its chomping. */
        private String blockScalar(String header, int indent) {
            boolean folded = header.charAt(0) == '>';
            boolean strip = header.contains("-");
            boolean keep = header.contains("+");

            List<String> body = new ArrayList<>();
            int block = -1;
            while (at < lines.size()) {
                Line line = lines.get(at);
                if (line.raw.isBlank()) {
                    body.add("");
                    at++;
                    continue;
                }
                if (line.indent <= indent) {
                    break;
                }
                if (block < 0) {
                    block = line.indent;
                }
                body.add(line.raw.length() >= block ? line.raw.substring(block) : "");
                at++;
            }
            while (!keep && !body.isEmpty() && body.get(body.size() - 1).isEmpty()) {
                body.remove(body.size() - 1);
            }

            StringBuilder out = new StringBuilder();
            for (int i = 0; i < body.size(); i++) {
                String text = body.get(i);
                if (i > 0) {
                    // Folding joins with a space, except across a blank line,
                    // which stays a break.
                    out.append(folded && !text.isEmpty() && !body.get(i - 1).isEmpty()
                        ? " " : "\n");
                }
                out.append(text);
            }
            if (!strip && !body.isEmpty()) {
                out.append('\n');
            }
            return out.toString();
        }
    }

    /** Where a mapping key ends, or -1 if this line is not a mapping entry.
     *
     * The colon has to be outside quotes and followed by a space or the end of
     * the line, because `http://example` and `12:30` are values, not keys.
     */
    private static int keyEnd(String content) {
        char quote = 0;
        for (int i = 0; i < content.length(); i++) {
            char c = content.charAt(i);
            if (quote != 0) {
                if (c == quote) {
                    quote = 0;
                }
                continue;
            }
            if (c == '\'' || c == '"') {
                quote = c;
            } else if (c == '#' && i > 0 && content.charAt(i - 1) == ' ') {
                return -1;
            } else if (c == ':' && (i + 1 == content.length() || content.charAt(i + 1) == ' ')) {
                return i;
            } else if ((c == '[' || c == '{') && i == 0) {
                return -1;
            }
        }
        return -1;
    }

    /** Drops a trailing comment, which only starts after whitespace. */
    private static String stripComment(String text) {
        char quote = 0;
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            if (quote != 0) {
                if (c == quote) {
                    quote = 0;
                }
            } else if (c == '\'' || c == '"') {
                quote = c;
            } else if (c == '#' && (i == 0 || text.charAt(i - 1) == ' ')) {
                return text.substring(0, i).stripTrailing();
            }
        }
        return text;
    }

    // scalars

    /** One value, given the type YAML 1.1 gives it. */
    static Object scalar(String text) {
        String value = text.strip();
        if (value.isEmpty() || value.equals("~")) {
            return null;
        }
        char first = value.charAt(0);
        if (first == '\'' || first == '"') {
            return unquote(value);
        }
        if (first == '[') {
            return flowSequence(value);
        }
        if (first == '{') {
            return flowMapping(value);
        }
        String lower = value.toLowerCase(Locale.ROOT);
        switch (lower) {
            case "null":
                return null;
            case "true":
            case "yes":
            case "on":
                return Boolean.TRUE;
            case "false":
            case "no":
            case "off":
                return Boolean.FALSE;
            default:
                break;
        }
        Object number = number(value);
        return number == null ? value : number;
    }

    private static Object number(String value) {
        if (!value.matches("[-+]?(\\d[\\d_]*(\\.[\\d_]*)?|\\.\\d[\\d_]*)([eE][-+]?\\d+)?")
            && !value.matches("0[xX][0-9a-fA-F_]+")) {
            return null;
        }
        String digits = value.replace("_", "");
        try {
            if (digits.matches("0[xX][0-9a-fA-F]+")) {
                return Long.decode(digits).intValue();
            }
            if (digits.indexOf('.') < 0 && digits.indexOf('e') < 0 && digits.indexOf('E') < 0) {
                long parsed = Long.parseLong(digits);
                // An int where it fits, because `getInt` is what plugins call
                // and `isInt` is what some of them check.
                return parsed >= Integer.MIN_VALUE && parsed <= Integer.MAX_VALUE
                    ? (Object) (int) parsed
                    : (Object) parsed;
            }
            return Double.valueOf(digits);
        } catch (NumberFormatException notANumber) {
            return null;
        }
    }

    private static String unquote(String value) {
        if (value.length() < 2) {
            return value;
        }
        char first = value.charAt(0);
        if (first != value.charAt(value.length() - 1) || (first != '\'' && first != '"')) {
            return value;
        }
        String body = value.substring(1, value.length() - 1);
        if (first == '\'') {
            // A single-quoted string escapes only its own quote, by doubling.
            return body.replace("''", "'");
        }
        StringBuilder out = new StringBuilder(body.length());
        for (int i = 0; i < body.length(); i++) {
            char c = body.charAt(i);
            if (c != '\\' || i + 1 == body.length()) {
                out.append(c);
                continue;
            }
            char next = body.charAt(++i);
            switch (next) {
                case 'n' -> out.append('\n');
                case 't' -> out.append('\t');
                case 'r' -> out.append('\r');
                case '0' -> out.append('\0');
                case 'u' -> {
                    if (i + 4 < body.length()) {
                        out.append((char) Integer.parseInt(body.substring(i + 1, i + 5), 16));
                        i += 4;
                    }
                }
                default -> out.append(next);
            }
        }
        return out.toString();
    }

    /** Splits a flow collection on commas that are not inside something. */
    private static List<String> flowParts(String text) {
        List<String> parts = new ArrayList<>();
        int depth = 0;
        char quote = 0;
        StringBuilder current = new StringBuilder();
        for (int i = 1; i < text.length() - 1; i++) {
            char c = text.charAt(i);
            if (quote != 0) {
                current.append(c);
                if (c == quote) {
                    quote = 0;
                }
                continue;
            }
            switch (c) {
                case '\'', '"' -> {
                    quote = c;
                    current.append(c);
                }
                case '[', '{' -> {
                    depth++;
                    current.append(c);
                }
                case ']', '}' -> {
                    depth--;
                    current.append(c);
                }
                case ',' -> {
                    if (depth == 0) {
                        parts.add(current.toString());
                        current.setLength(0);
                    } else {
                        current.append(c);
                    }
                }
                default -> current.append(c);
            }
        }
        if (!current.toString().isBlank()) {
            parts.add(current.toString());
        }
        return parts;
    }

    private static List<Object> flowSequence(String text) {
        List<Object> out = new ArrayList<>();
        for (String part : flowParts(text)) {
            out.add(scalar(part));
        }
        return out;
    }

    private static Map<String, Object> flowMapping(String text) {
        Map<String, Object> out = new LinkedHashMap<>();
        for (String part : flowParts(text)) {
            String entry = part.strip();
            int colon = keyEnd(entry);
            if (colon < 0) {
                out.put(unquote(entry), null);
            } else {
                out.put(unquote(entry.substring(0, colon).strip()),
                    scalar(entry.substring(colon + 1)));
            }
        }
        return out;
    }

    // write

    @SuppressWarnings("unchecked")
    private static void writeNode(StringBuilder out, Object value, int indent, boolean inline) {
        if (value instanceof Map) {
            writeMapping(out, (Map<Object, Object>) value, indent, inline);
        } else if (value instanceof List) {
            writeSequence(out, (List<Object>) value, indent, inline);
        } else {
            out.append(inline ? " " : "").append(writeScalar(value)).append('\n');
        }
    }

    private static void writeMapping(
            StringBuilder out, Map<Object, Object> map, int indent, boolean inline) {
        if (map.isEmpty()) {
            out.append(inline ? " {}\n" : "{}\n");
            return;
        }
        if (inline) {
            out.append('\n');
        }
        String pad = " ".repeat(indent);
        for (Map.Entry<Object, Object> entry : map.entrySet()) {
            out.append(pad).append(writeKey(String.valueOf(entry.getKey()))).append(':');
            writeNode(out, entry.getValue(), indent + 2, true);
        }
    }

    private static void writeSequence(
            StringBuilder out, List<Object> list, int indent, boolean inline) {
        if (list.isEmpty()) {
            out.append(inline ? " []\n" : "[]\n");
            return;
        }
        if (inline) {
            out.append('\n');
        }
        // A sequence sits at its key's own indent, which is what SnakeYAML
        // writes and what every hand-edited plugin config looks like.
        String pad = " ".repeat(Math.max(0, indent - 2));
        for (Object item : list) {
            out.append(pad).append('-');
            if (item instanceof Map || item instanceof List) {
                StringBuilder nested = new StringBuilder();
                writeNode(nested, item, indent, false);
                String text = nested.toString();
                // The first line joins the dash; the rest keep their indent.
                out.append(' ').append(text.substring(indent).stripTrailing()).append('\n');
                int newline = text.indexOf('\n');
                if (newline >= 0 && newline + 1 < text.length()) {
                    out.append(text.substring(newline + 1));
                }
            } else {
                out.append(' ').append(writeScalar(item)).append('\n');
            }
        }
    }

    private static String writeKey(String key) {
        return needsQuoting(key) ? quote(key) : key;
    }

    private static String writeScalar(Object value) {
        if (value == null) {
            return "null";
        }
        if (value instanceof Boolean || value instanceof Number) {
            return String.valueOf(value);
        }
        String text = String.valueOf(value);
        return needsQuoting(text) ? quote(text) : text;
    }

    /** Whether reading this back plainly would give something else. */
    private static boolean needsQuoting(String text) {
        if (text.isEmpty() || !text.equals(text.strip())) {
            return true;
        }
        if (!(scalar(text) instanceof String)) {
            // It would come back as a number, a boolean or null.
            return true;
        }
        if (text.indexOf('\n') >= 0 || text.indexOf(": ") >= 0 || text.endsWith(":")
            || text.contains(" #")) {
            return true;
        }
        return "-?:,[]{}#&*!|>'\"%@`".indexOf(text.charAt(0)) >= 0;
    }

    private static String quote(String text) {
        if (text.indexOf('\n') >= 0) {
            return '"' + text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n")
                + '"';
        }
        return "'" + text.replace("'", "''") + "'";
    }
}
