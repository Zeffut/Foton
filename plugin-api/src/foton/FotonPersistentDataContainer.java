package foton;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/** Typed container whose supported item values persist through vanilla custom data. */
public final class FotonPersistentDataContainer implements PersistentDataContainer {
    private static final int MAX_SERIALIZED_KEY_BYTES = 1024;
    private static final int MAX_NBT_UTF_BYTES = 65_535;
    private static final int MAX_SERIALIZED_ENTRIES = 1_024;
    private static final int MAX_ENCODED_ITEM_PDC_CHARS = 4_194_304;

    private static final class StoredValue {
        private final PersistentDataType<?, ?> type;
        private final Object value;

        private StoredValue(PersistentDataType<?, ?> type, Object value) {
            this.type = type;
            this.value = copyValue(value);
        }

        private StoredValue copy() {
            return new StoredValue(type, value);
        }

        @Override public boolean equals(Object other) {
            if (!(other instanceof StoredValue stored) || type != stored.type) return false;
            if (value instanceof byte[] bytes && stored.value instanceof byte[] otherBytes)
                return java.util.Arrays.equals(bytes, otherBytes);
            return java.util.Objects.equals(value, stored.value);
        }

        @Override public int hashCode() {
            return 31 * System.identityHashCode(type)
                + (value instanceof byte[] bytes ? java.util.Arrays.hashCode(bytes) : value.hashCode());
        }
    }

    private final Map<NamespacedKey, StoredValue> values = new HashMap<>();
    private final Set<NamespacedKey> nativeExposedKeys = new HashSet<>();
    private String nativePassthrough;
    private String nativePassthroughIdentity;
    private Runnable mutationListener = () -> { };

    /** Internal persistence guard: projected keys alone do not describe the retained raw state. */
    public boolean hasRetainedItemState() {
        return !values.isEmpty() || nativePassthrough != null || nativePassthroughIdentity != null;
    }

    /** Internal item-owner notification; copies are rebound to their new owner. */
    public void setMutationListener(Runnable listener) {
        mutationListener = java.util.Objects.requireNonNull(listener);
    }

    private static Object copyValue(Object value) {
        return value instanceof byte[] bytes ? bytes.clone() : value;
    }

    @Override public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (key == null || type == null) throw new IllegalArgumentException("key and type are required");
        if (value == null) { remove(key); return; }
        values.put(key, new StoredValue(type, value));
        mutationListener.run();
    }
    @SuppressWarnings("unchecked")
    @Override public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        StoredValue stored = values.get(key);
        return stored == null || stored.type != type ? null : (C) copyValue(stored.value);
    }
    @Override public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C fallback) {
        C value = get(key, type);
        return value == null ? fallback : value;
    }
    @Override public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        StoredValue stored = values.get(key);
        return stored != null && stored.type == type;
    }
    @Override public boolean has(NamespacedKey key) { return values.containsKey(key); }
    @Override public void remove(NamespacedKey key) {
        if (values.remove(key) != null) mutationListener.run();
    }
    @Override public Set<NamespacedKey> getKeys() { return new HashSet<>(values.keySet()); }
    public FotonPersistentDataContainer copy() {
        FotonPersistentDataContainer copy = new FotonPersistentDataContainer();
        values.forEach((key, value) -> copy.values.put(key, value.copy()));
        copy.nativeExposedKeys.addAll(nativeExposedKeys);
        copy.nativePassthrough = nativePassthrough;
        copy.nativePassthroughIdentity = nativePassthroughIdentity;
        return copy;
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
        java.util.ArrayList<Map.Entry<NamespacedKey, StoredValue>> entries =
            new java.util.ArrayList<>(values.entrySet());
        entries.sort(java.util.Map.Entry.comparingByKey(
            java.util.Comparator.comparing(NamespacedKey::toString)));
        for (Map.Entry<NamespacedKey, StoredValue> entry : entries) {
            StoredValue stored = entry.getValue();
            String prefix;
            String value;
            if (stored.type == PersistentDataType.STRING && stored.value instanceof String string) {
                prefix = "pdcstrhex=";
                value = strictHexEncode(string, MAX_NBT_UTF_BYTES);
            } else if (stored.type == PersistentDataType.BYTE && stored.value instanceof Byte byteValue) {
                prefix = "pdcbyte=";
                value = byteValue.toString();
            } else if (stored.type == PersistentDataType.INTEGER && stored.value instanceof Integer intValue) {
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
        values.put(key, new StoredValue(type, value));
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
    @Override public boolean equals(Object other) {
        return other instanceof FotonPersistentDataContainer container
            && values.equals(container.values)
            && java.util.Objects.equals(nativePassthroughIdentity,
                container.nativePassthroughIdentity);
    }
    @Override public int hashCode() {
        return 31 * values.hashCode() + java.util.Objects.hashCode(nativePassthroughIdentity);
    }
}
