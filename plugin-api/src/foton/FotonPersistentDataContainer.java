package foton;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataAdapterContext;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/** Typed custom data, held as the NBT primitives Paper stores it as.
 *
 * <p>Each value is kept as what its {@link PersistentDataType} turned it into,
 * so reading it back with another type answers as Paper does (an
 * {@link IllegalArgumentException}), and the whole container converts to NBT
 * -- which is how an item's data reaches the server's {@code custom_data}
 * component under {@code PublicBukkitValues}, and survives there.</p>
 */
public class FotonPersistentDataContainer implements PersistentDataContainer {
    private static final int MAX_SERIALIZED_KEY_BYTES = 1024;
    private static final int MAX_NBT_UTF_BYTES = 65_535;
    private static final int MAX_SERIALIZED_ENTRIES = 1_024;
    private static final int MAX_ENCODED_ITEM_PDC_CHARS = 4_194_304;


    private final Set<NamespacedKey> nativeExposedKeys = new LinkedHashSet<>();
    private String nativePassthrough;
    private String nativePassthroughIdentity;
    private Runnable mutationListener = () -> {};
    public boolean hasRetainedItemState() {
        return !values.isEmpty() || !opaqueValues.isEmpty() || nativePassthrough != null || nativePassthroughIdentity != null;
    }
    public void setMutationListener(Runnable listener) {
        mutationListener = java.util.Objects.requireNonNull(listener);
    }

    public static final PersistentDataAdapterContext CONTEXT = FotonPersistentDataContainer::new;

    private final Map<NamespacedKey, Object> values = new LinkedHashMap<>();
    // Unaddressable native compound entries survive edits to valid PDC keys.
    private final Map<String, Object> opaqueValues = new LinkedHashMap<>();

    @Override
    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (key == null || type == null) throw new IllegalArgumentException("key and type are required");
        if (value == null) { remove(key); return; }
        P primitive = type.toPrimitive(value, CONTEXT);
        if (primitive == null) throw new IllegalArgumentException(type + " turned a value into null");
        values.put(key, copyPrimitive(primitive));
        mutationListener.run();
    }

    @Override
    public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        Object stored = values.get(key);
        if (stored == null) return null;
        if (!type.getPrimitiveType().isInstance(stored)) {
            throw new IllegalArgumentException("The value under " + key + " is a "
                + stored.getClass().getSimpleName() + ", which cannot be read as "
                + type.getPrimitiveType().getSimpleName());
        }
        return type.fromPrimitive(type.getPrimitiveType().cast(copyPrimitive(stored)), CONTEXT);
    }

    @Override
    public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C fallback) {
        C value = get(key, type);
        return value == null ? fallback : value;
    }

    @Override
    public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        Object stored = values.get(key);
        return stored != null && type.getPrimitiveType().isInstance(stored);
    }

    @Override public boolean has(NamespacedKey key) { return values.containsKey(key); }
    @Override public void remove(NamespacedKey key) { if (values.remove(key) != null || nativeExposedKeys.contains(key)) mutationListener.run(); }
    @Override public Set<NamespacedKey> getKeys() { return new LinkedHashSet<>(values.keySet()); }
    @Override public boolean isEmpty() { return values.isEmpty(); }
    @Override public int getSize() { return values.size(); }
    @Override public PersistentDataAdapterContext getAdapterContext() { return CONTEXT; }

    @Override
    public void copyTo(PersistentDataContainer other, boolean replace) {
        if (!(other instanceof FotonPersistentDataContainer target)) {
            throw new IllegalArgumentException("not a Foton container: " + other);
        }
        for (Map.Entry<NamespacedKey, Object> entry : values.entrySet()) {
            if (replace || !target.values.containsKey(entry.getKey())) {
                target.values.put(entry.getKey(), copyPrimitive(entry.getValue()));
                target.mutationListener.run();
            }
        }
    }

    @Override
    public byte[] serializeToBytes() throws java.io.IOException {
        return Snbt.toBinary(toNbt());
    }

    @Override
    public void readFromBytes(byte[] bytes, boolean clear) throws java.io.IOException {
        FotonPersistentDataContainer read = fromNbt(Snbt.fromBinary(bytes));
        if (clear) { values.clear(); opaqueValues.clear(); }
        values.putAll(read.values);
        opaqueValues.putAll(read.opaqueValues);
        mutationListener.run();
    }

    public FotonPersistentDataContainer copy() {
        FotonPersistentDataContainer copy = new FotonPersistentDataContainer();
        copyTo(copy, true);
        copy.nativeExposedKeys.addAll(nativeExposedKeys);
        copy.nativePassthrough = nativePassthrough;
        copy.nativePassthroughIdentity = nativePassthroughIdentity;
        opaqueValues.forEach((key, value) -> copy.opaqueValues.put(key, copyPrimitive(value)));
        return copy;
    }

    /** The container as an NBT compound, keys sorted so equal containers write equal text. */
    public Map<String, Object> toNbt() {
        Map<String, Object> out = new TreeMap<>();
        opaqueValues.forEach((key, value) -> out.put(key, toNbtValue(value)));
        for (Map.Entry<NamespacedKey, Object> entry : values.entrySet()) {
            out.put(entry.getKey().toString(), toNbtValue(entry.getValue()));
        }
        return out;
    }

    /** Reads valid keys into the API view; unaddressable entries remain opaque. */
    @SuppressWarnings("unchecked")
    public static FotonPersistentDataContainer fromNbt(Map<String, Object> compound) {
        FotonPersistentDataContainer container = new FotonPersistentDataContainer();
        for (Map.Entry<String, Object> entry : compound.entrySet()) {
            NamespacedKey key = entry.getKey().indexOf(':') < 0 ? null : NamespacedKey.fromString(entry.getKey());
            if (key != null && key.toString().equals(entry.getKey())) {
                container.values.put(key, fromNbtValue(entry.getValue()));
                container.nativeExposedKeys.add(key);
            } else container.opaqueValues.put(entry.getKey(), fromNbtValue(entry.getValue()));
        }
        return container;
    }

    private static Object toNbtValue(Object primitive) {
        if (primitive instanceof FotonPersistentDataContainer nested) return nested.toNbt();
        if (primitive instanceof PersistentDataContainer[] nested) {
            List<Object> list = new ArrayList<>();
            for (PersistentDataContainer element : nested) list.add(((FotonPersistentDataContainer) element).toNbt());
            return list;
        }
        if (primitive instanceof List<?> list) {
            List<Object> out = new ArrayList<>();
            for (Object element : list) out.add(toNbtValue(element));
            return out;
        }
        return primitive;
    }

    @SuppressWarnings("unchecked")
    private static Object fromNbtValue(Object nbt) {
        if (nbt instanceof Map<?, ?> map) return fromNbt((Map<String, Object>) map);
        if (nbt instanceof List<?> list) {
            if (!list.isEmpty() && list.stream().allMatch(element -> element instanceof Map<?, ?>)) {
                PersistentDataContainer[] out = new PersistentDataContainer[list.size()];
                for (int i = 0; i < out.length; i++) out[i] = fromNbt((Map<String, Object>) list.get(i));
                return out;
            }
            List<Object> out = new ArrayList<>();
            for (Object element : list) out.add(fromNbtValue(element));
            return out;
        }
        return nbt;
    }

    /** Arrays and containers are mutable; neither a caller nor the container may alias the other's. */
    private static Object copyPrimitive(Object value) {
        return switch (value) {
            case byte[] bytes -> bytes.clone();
            case int[] ints -> ints.clone();
            case long[] longs -> longs.clone();
            case FotonPersistentDataContainer nested -> nested.copy();
            case PersistentDataContainer[] nested -> {
                PersistentDataContainer[] out = new PersistentDataContainer[nested.length];
                for (int i = 0; i < out.length; i++) out[i] = ((FotonPersistentDataContainer) nested[i]).copy();
                yield out;
            }
            case Byte b -> b;
            case Short s -> s;
            case Integer i -> i;
            case Long l -> l;
            case Float f -> f;
            case Double d -> d;
            case String s -> s;
            case List<?> list -> {
                List<Object> out = new ArrayList<>();
                for (Object element : list) out.add(copyPrimitive(element));
                yield out;
            }
            default -> throw new IllegalArgumentException("a container cannot store a "
                + value.getClass().getName() + "; its PersistentDataType must produce an NBT primitive");
        };
    }

    String nativePassthrough() { return nativePassthrough; }
    void setNativePassthrough(String value) { nativePassthrough = value; }
    String nativePassthroughIdentity() { return nativePassthroughIdentity; }
    void setNativePassthroughIdentity(String value) { nativePassthroughIdentity = value; }

    /** Appends the subset backed by vanilla NBT primitives to the item bridge. */
    String encodeItemFields() {
        if (values.size() > MAX_SERIALIZED_ENTRIES)
            throw new IllegalArgumentException("too many persistent data entries for an item");
        StringBuilder encoded = new StringBuilder();
        java.util.ArrayList<Map.Entry<NamespacedKey, Object>> entries =
            new java.util.ArrayList<>(values.entrySet());
        entries.sort(java.util.Map.Entry.comparingByKey(
            java.util.Comparator.comparing(NamespacedKey::toString)));
        for (Map.Entry<NamespacedKey, Object> entry : entries) {
            Object stored = entry.getValue();
            String prefix;
            String value;
            if (stored instanceof String string) {
                prefix = "pdcstrhex=";
                value = strictHexEncode(string, MAX_NBT_UTF_BYTES);
            } else if (stored instanceof Byte byteValue) {
                prefix = "pdcbyte=";
                value = byteValue.toString();
            } else if (stored instanceof Integer intValue) {
                prefix = "pdcint=";
                value = intValue.toString();
            } else {
                continue;
            }
            String field = prefix
                + strictHexEncode(entry.getKey().toString(), MAX_SERIALIZED_KEY_BYTES)
                + ':' + value;
            appendBridgeField(encoded, field);
        }
        java.util.ArrayList<NamespacedKey> removed = nativeExposedKeys.stream()
            .filter(key -> !values.containsKey(key))
            .sorted(java.util.Comparator.comparing(NamespacedKey::toString))
            .collect(java.util.stream.Collectors.toCollection(java.util.ArrayList::new));
        for (NamespacedKey key : removed)
            appendBridgeField(encoded, "pdcremove="
                + strictHexEncode(key.toString(), MAX_SERIALIZED_KEY_BYTES));
        return encoded.toString();
    }

    private static void appendBridgeField(StringBuilder encoded, String field) {
        if (encoded.length() + field.length() + 1 > MAX_ENCODED_ITEM_PDC_CHARS)
            throw new IllegalArgumentException("persistent data exceeds the item bridge limit");
        encoded.append('\u001d').append(field);
    }

    /** Reads one item-bridge field, rejecting malformed or oversized values. */
    boolean decodeItemField(String field) {
        PersistentDataType<?, ?> type;
        int prefixLength;
        if (field.startsWith("pdcstrhex=")) {
            type = PersistentDataType.STRING;
            prefixLength = 10;
            if (field.length() > prefixLength + MAX_SERIALIZED_KEY_BYTES * 2 + 1
                    + MAX_NBT_UTF_BYTES * 2) return true;
        } else if (field.startsWith("pdcbyte=")) {
            type = PersistentDataType.BYTE;
            prefixLength = 8;
            if (field.length() > prefixLength + MAX_SERIALIZED_KEY_BYTES * 2 + 1 + 4)
                return true;
        } else if (field.startsWith("pdcint=")) {
            type = PersistentDataType.INTEGER;
            prefixLength = 7;
            if (field.length() > prefixLength + MAX_SERIALIZED_KEY_BYTES * 2 + 1 + 11)
                return true;
        } else {
            return false;
        }
        String payload = field.substring(prefixLength);
        int separator = payload.indexOf(':');
        if (separator <= 0) return true;
        byte[] keyBytes = hexDecode(payload.substring(0, separator), MAX_SERIALIZED_KEY_BYTES);
        if (keyBytes == null) return true;
        String decodedKey = decodeUtf8(keyBytes);
        if (decodedKey == null) return true;
        NamespacedKey key = NamespacedKey.fromString(decodedKey);
        if (key == null) return true;
        String value = payload.substring(separator + 1);
        try {
            if (type == PersistentDataType.STRING) {
                byte[] bytes = hexDecode(value, MAX_NBT_UTF_BYTES);
                String decoded = bytes == null ? null : decodeUtf8(bytes);
                if (decoded != null && modifiedUtf8Length(decoded) <= MAX_NBT_UTF_BYTES)
                    setUntyped(key, type, decoded);
            } else if (type == PersistentDataType.BYTE) {
                setUntyped(key, type, Byte.valueOf(value));
            } else {
                setUntyped(key, type, Integer.valueOf(value));
            }
        } catch (NumberFormatException ignored) {
            // A malformed optional metadata field must not make the item unreadable.
        }
        if (values.containsKey(key)
                && (nativeExposedKeys.contains(key)
                    || nativeExposedKeys.size() < MAX_SERIALIZED_ENTRIES)) nativeExposedKeys.add(key);
        return true;
    }

    private void setUntyped(NamespacedKey key, PersistentDataType<?, ?> type, Object value) {
        values.put(key, copyPrimitive(value));
    }

    private static String strictHexEncode(String value, int maximumBytes) {
        if (modifiedUtf8Length(value) > maximumBytes)
            throw new IllegalArgumentException("persistent data exceeds the NBT UTF limit");
        byte[] bytes;
        try {
            java.nio.ByteBuffer encoded = java.nio.charset.StandardCharsets.UTF_8.newEncoder()
                .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
                .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
                .encode(java.nio.CharBuffer.wrap(value));
            bytes = new byte[encoded.remaining()];
            encoded.get(bytes);
        } catch (java.nio.charset.CharacterCodingException malformed) {
            throw new IllegalArgumentException("persistent data contains malformed UTF-16", malformed);
        }
        StringBuilder hex = new StringBuilder();
        for (byte byteValue : bytes)
            hex.append(String.format(java.util.Locale.ROOT, "%02x", byteValue & 0xff));
        return hex.toString();
    }

    private static int modifiedUtf8Length(String value) {
        int length = 0;
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            length += character == 0 ? 2 : character <= 0x7f ? 1 : character <= 0x7ff ? 2 : 3;
            if (length > MAX_NBT_UTF_BYTES) return length;
        }
        return length;
    }

    private static byte[] hexDecode(String value, int maximumBytes) {
        if ((value.length() & 1) != 0 || value.length() / 2 > maximumBytes) return null;
        byte[] decoded = new byte[value.length() / 2];
        for (int index = 0; index < decoded.length; index++) {
            int high = Character.digit(value.charAt(index * 2), 16);
            int low = Character.digit(value.charAt(index * 2 + 1), 16);
            if (high < 0 || low < 0) return null;
            decoded[index] = (byte) ((high << 4) | low);
        }
        return decoded;
    }

    private static String decodeUtf8(byte[] value) {
        try {
            return java.nio.charset.StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(java.nio.charset.CodingErrorAction.REPORT)
                .onUnmappableCharacter(java.nio.charset.CodingErrorAction.REPORT)
                .decode(java.nio.ByteBuffer.wrap(value)).toString();
        } catch (java.nio.charset.CharacterCodingException malformed) {
            return null;
        }
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonPersistentDataContainer container
            && Snbt.write(toNbt()).equals(Snbt.write(container.toNbt()))
            && java.util.Objects.equals(nativePassthroughIdentity, container.nativePassthroughIdentity);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(Snbt.write(toNbt()), nativePassthroughIdentity);
    }

    @Override
    public String toString() {
        return Snbt.write(toNbt());
    }
}
