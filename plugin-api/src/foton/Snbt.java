package foton;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** NBT as SNBT text, in the plain Java types a persistent data container holds.
 *
 * <p>A compound is a {@code Map<String, Object>}, a list a {@code List}, and
 * the rest {@code Byte}, {@code Short}, {@code Integer}, {@code Long},
 * {@code Float}, {@code Double}, {@code String}, {@code byte[]},
 * {@code int[]} and {@code long[]} -- one Java type per NBT type, so a value
 * read back is the type it was written as.</p>
 *
 * <p>Item custom data crosses to Rust in this form. The writer produces what
 * foton-utils' SNBT parser accepts, and the reader accepts what its canonical
 * writer produces.</p>
 */
public final class Snbt {
    private Snbt() { }

    public static String write(Object value) {
        StringBuilder out = new StringBuilder();
        write(value, out);
        return out.toString();
    }

    @SuppressWarnings("unchecked")
    private static void write(Object value, StringBuilder out) {
        if (value instanceof Map<?, ?> map) {
            out.append('{');
            boolean first = true;
            for (Map.Entry<String, Object> entry : ((Map<String, Object>) map).entrySet()) {
                if (!first) out.append(',');
                first = false;
                quote(entry.getKey(), out);
                out.append(':');
                write(entry.getValue(), out);
            }
            out.append('}');
        } else if (value instanceof List<?> list) {
            out.append('[');
            for (int index = 0; index < list.size(); index++) {
                if (index != 0) out.append(',');
                write(list.get(index), out);
            }
            out.append(']');
        } else if (value instanceof Byte b) {
            out.append(b).append('b');
        } else if (value instanceof Short s) {
            out.append(s).append('s');
        } else if (value instanceof Integer i) {
            out.append(i);
        } else if (value instanceof Long l) {
            out.append(l).append('L');
        } else if (value instanceof Float f) {
            out.append(f).append('f');
        } else if (value instanceof Double d) {
            out.append(d).append('d');
        } else if (value instanceof String s) {
            quote(s, out);
        } else if (value instanceof byte[] bytes) {
            out.append("[B;");
            for (int index = 0; index < bytes.length; index++) out.append(index == 0 ? "" : ",").append(bytes[index]).append('B');
            out.append(']');
        } else if (value instanceof int[] ints) {
            out.append("[I;");
            for (int index = 0; index < ints.length; index++) out.append(index == 0 ? "" : ",").append(ints[index]);
            out.append(']');
        } else if (value instanceof long[] longs) {
            out.append("[L;");
            for (int index = 0; index < longs.length; index++) out.append(index == 0 ? "" : ",").append(longs[index]).append('L');
            out.append(']');
        } else {
            throw new IllegalArgumentException("not an NBT value: " + value);
        }
    }

    private static void quote(String value, StringBuilder out) {
        out.append('"');
        for (int index = 0; index < value.length(); index++) {
            char c = value.charAt(index);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\t' -> out.append("\\t");
                case '\r' -> out.append("\\r");
                case '\b' -> out.append("\\b");
                case '\f' -> out.append("\\f");
                default -> {
                    if (c < ' ') out.append(String.format("\\x%02x", (int) c));
                    else out.append(c);
                }
            }
        }
        out.append('"');
    }

    /** A compound as binary NBT: an unnamed root compound, as vanilla's NbtIo writes one. */
    public static byte[] toBinary(Map<String, Object> compound) throws java.io.IOException {
        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
        java.io.DataOutputStream out = new java.io.DataOutputStream(bytes);
        out.writeByte(10);
        out.writeUTF("");
        writeBinary(compound, out);
        out.flush();
        return bytes.toByteArray();
    }

    /** Reads what {@link #toBinary} writes. */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> fromBinary(byte[] data) throws java.io.IOException {
        java.io.DataInputStream in = new java.io.DataInputStream(new java.io.ByteArrayInputStream(data));
        if (in.readByte() != 10) throw new java.io.IOException("the root tag is not a compound");
        in.readUTF();
        return (Map<String, Object>) readBinary(10, in, 0);
    }

    private static int typeId(Object value) {
        if (value instanceof Byte) return 1;
        if (value instanceof Short) return 2;
        if (value instanceof Integer) return 3;
        if (value instanceof Long) return 4;
        if (value instanceof Float) return 5;
        if (value instanceof Double) return 6;
        if (value instanceof byte[]) return 7;
        if (value instanceof String) return 8;
        if (value instanceof List<?>) return 9;
        if (value instanceof Map<?, ?>) return 10;
        if (value instanceof int[]) return 11;
        if (value instanceof long[]) return 12;
        throw new IllegalArgumentException("not an NBT value: " + value);
    }

    @SuppressWarnings("unchecked")
    private static void writeBinary(Object value, java.io.DataOutputStream out) throws java.io.IOException {
        switch (value) {
            case Byte b -> out.writeByte(b);
            case Short s -> out.writeShort(s);
            case Integer i -> out.writeInt(i);
            case Long l -> out.writeLong(l);
            case Float f -> out.writeFloat(f);
            case Double d -> out.writeDouble(d);
            case byte[] bytes -> { out.writeInt(bytes.length); out.write(bytes); }
            case String s -> out.writeUTF(s);
            case List<?> list -> {
                out.writeByte(list.isEmpty() ? 0 : typeId(list.get(0)));
                out.writeInt(list.size());
                for (Object element : list) writeBinary(element, out);
            }
            case Map<?, ?> map -> {
                for (Map.Entry<String, Object> entry : ((Map<String, Object>) map).entrySet()) {
                    out.writeByte(typeId(entry.getValue()));
                    out.writeUTF(entry.getKey());
                    writeBinary(entry.getValue(), out);
                }
                out.writeByte(0);
            }
            case int[] ints -> { out.writeInt(ints.length); for (int i : ints) out.writeInt(i); }
            case long[] longs -> { out.writeInt(longs.length); for (long l : longs) out.writeLong(l); }
            default -> throw new IllegalArgumentException("not an NBT value: " + value);
        }
    }

    private static Object readBinary(int type, java.io.DataInputStream in, int depth) throws java.io.IOException {
        // Vanilla's NbtAccounter ceiling: deeper than this is an attack, not data.
        if (depth > 512) throw new java.io.IOException("NBT nested deeper than 512");
        switch (type) {
            case 1: return in.readByte();
            case 2: return in.readShort();
            case 3: return in.readInt();
            case 4: return in.readLong();
            case 5: return in.readFloat();
            case 6: return in.readDouble();
            case 7: { byte[] out = new byte[length(in)]; in.readFully(out); return out; }
            case 8: return in.readUTF();
            case 9: {
                int element = in.readByte();
                int size = length(in);
                List<Object> out = new ArrayList<>();
                for (int i = 0; i < size; i++) out.add(readBinary(element, in, depth + 1));
                return out;
            }
            case 10: {
                Map<String, Object> out = new LinkedHashMap<>();
                while (true) {
                    int element = in.readByte();
                    if (element == 0) return out;
                    String name = in.readUTF();
                    out.put(name, readBinary(element, in, depth + 1));
                }
            }
            case 11: { int[] out = new int[length(in)]; for (int i = 0; i < out.length; i++) out[i] = in.readInt(); return out; }
            case 12: { long[] out = new long[length(in)]; for (int i = 0; i < out.length; i++) out[i] = in.readLong(); return out; }
            default: throw new java.io.IOException("unknown NBT tag type " + type);
        }
    }

    private static int length(java.io.DataInputStream in) throws java.io.IOException {
        int length = in.readInt();
        if (length < 0 || length > in.available() + 1) throw new java.io.IOException("NBT length " + length + " exceeds the data");
        return length;
    }

    /** Parses one compound; throws {@link IllegalArgumentException} on anything else. */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> parseCompound(String text) {
        Reader reader = new Reader(text);
        Object value = reader.value();
        reader.skipSpace();
        if (!(value instanceof Map<?, ?>) || reader.index != text.length()) {
            throw new IllegalArgumentException("not one SNBT compound: " + text);
        }
        return (Map<String, Object>) value;
    }

    private static final class Reader {
        private final String text;
        private int index;

        Reader(String text) {
            this.text = text;
        }

        void skipSpace() {
            while (index < text.length() && Character.isWhitespace(text.charAt(index))) index++;
        }

        char peek() {
            skipSpace();
            if (index >= text.length()) throw new IllegalArgumentException("SNBT ends early");
            return text.charAt(index);
        }

        void expect(char c) {
            if (peek() != c) throw new IllegalArgumentException("expected '" + c + "' at " + index + " in " + text);
            index++;
        }

        Object value() {
            char c = peek();
            if (c == '{') return compound();
            if (c == '[') return list();
            if (c == '"' || c == '\'') return quoted();
            return scalar(bare());
        }

        Map<String, Object> compound() {
            expect('{');
            Map<String, Object> map = new LinkedHashMap<>();
            if (peek() == '}') { index++; return map; }
            while (true) {
                String key = peek() == '"' || peek() == '\'' ? quoted() : bare();
                expect(':');
                map.put(key, value());
                if (peek() == ',') { index++; continue; }
                expect('}');
                return map;
            }
        }

        Object list() {
            expect('[');
            if (index + 1 < text.length() && text.charAt(index + 1) == ';') {
                char kind = text.charAt(index);
                index += 2;
                List<String> parts = new ArrayList<>();
                if (peek() != ']') {
                    while (true) {
                        parts.add(bare());
                        if (peek() == ',') { index++; continue; }
                        break;
                    }
                }
                expect(']');
                return typedArray(kind, parts);
            }
            List<Object> list = new ArrayList<>();
            if (peek() == ']') { index++; return list; }
            while (true) {
                list.add(value());
                if (peek() == ',') { index++; continue; }
                expect(']');
                return list;
            }
        }

        Object typedArray(char kind, List<String> parts) {
            switch (kind) {
                case 'B' -> {
                    byte[] out = new byte[parts.size()];
                    for (int i = 0; i < out.length; i++) out[i] = ((Number) scalar(parts.get(i))).byteValue();
                    return out;
                }
                case 'I' -> {
                    int[] out = new int[parts.size()];
                    for (int i = 0; i < out.length; i++) out[i] = ((Number) scalar(parts.get(i))).intValue();
                    return out;
                }
                case 'L' -> {
                    long[] out = new long[parts.size()];
                    for (int i = 0; i < out.length; i++) out[i] = ((Number) scalar(parts.get(i))).longValue();
                    return out;
                }
                default -> throw new IllegalArgumentException("unknown array type " + kind);
            }
        }

        String quoted() {
            char quote = text.charAt(index++);
            StringBuilder out = new StringBuilder();
            while (index < text.length()) {
                char c = text.charAt(index++);
                if (c == quote) return out.toString();
                if (c != '\\') { out.append(c); continue; }
                char escaped = text.charAt(index++);
                switch (escaped) {
                    case 'n' -> out.append('\n');
                    case 't' -> out.append('\t');
                    case 'r' -> out.append('\r');
                    case 'b' -> out.append('\b');
                    case 'f' -> out.append('\f');
                    case 's' -> out.append(' ');
                    case 'x' -> { out.append((char) Integer.parseInt(text.substring(index, index + 2), 16)); index += 2; }
                    case 'u' -> { out.append((char) Integer.parseInt(text.substring(index, index + 4), 16)); index += 4; }
                    default -> out.append(escaped);
                }
            }
            throw new IllegalArgumentException("unterminated string in " + text);
        }

        String bare() {
            skipSpace();
            int start = index;
            while (index < text.length()) {
                char c = text.charAt(index);
                if (Character.isLetterOrDigit(c) || c == '.' || c == '_' || c == '+' || c == '-') index++;
                else break;
            }
            if (start == index) throw new IllegalArgumentException("expected a value at " + index + " in " + text);
            return text.substring(start, index);
        }

        Object scalar(String token) {
            String lower = token.toLowerCase(java.util.Locale.ROOT);
            if (lower.equals("true")) return (byte) 1;
            if (lower.equals("false")) return (byte) 0;
            try {
                char suffix = lower.charAt(lower.length() - 1);
                String number = token.substring(0, token.length() - 1);
                switch (suffix) {
                    case 'b': return Byte.parseByte(number);
                    case 's': return Short.parseShort(number);
                    case 'l': return Long.parseLong(number);
                    case 'f': return Float.parseFloat(number);
                    case 'd': return Double.parseDouble(number);
                    default: break;
                }
                if (lower.contains(".") || lower.contains("e")) return Double.parseDouble(token);
                return Integer.parseInt(token);
            } catch (NumberFormatException notANumber) {
                return token;
            }
        }
    }
}
