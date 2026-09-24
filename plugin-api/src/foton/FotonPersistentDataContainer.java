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
public final class FotonPersistentDataContainer implements PersistentDataContainer {
    public static final PersistentDataAdapterContext CONTEXT = FotonPersistentDataContainer::new;

    private final Map<NamespacedKey, Object> values = new LinkedHashMap<>();

    @Override
    public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (key == null || type == null) throw new IllegalArgumentException("key and type are required");
        if (value == null) { values.remove(key); return; }
        P primitive = type.toPrimitive(value, CONTEXT);
        if (primitive == null) throw new IllegalArgumentException(type + " turned a value into null");
        values.put(key, copyPrimitive(primitive));
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
    @Override public void remove(NamespacedKey key) { values.remove(key); }
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
        if (clear) values.clear();
        values.putAll(read.values);
    }

    public FotonPersistentDataContainer copy() {
        FotonPersistentDataContainer copy = new FotonPersistentDataContainer();
        copyTo(copy, true);
        return copy;
    }

    /** The container as an NBT compound, keys sorted so equal containers write equal text. */
    public Map<String, Object> toNbt() {
        Map<String, Object> out = new TreeMap<>();
        for (Map.Entry<NamespacedKey, Object> entry : values.entrySet()) {
            out.put(entry.getKey().toString(), toNbtValue(entry.getValue()));
        }
        return out;
    }

    /** Reads what {@link #toNbt} writes. Keys that are not namespaced are skipped, as Paper skips them. */
    @SuppressWarnings("unchecked")
    public static FotonPersistentDataContainer fromNbt(Map<String, Object> compound) {
        FotonPersistentDataContainer container = new FotonPersistentDataContainer();
        for (Map.Entry<String, Object> entry : compound.entrySet()) {
            if (entry.getKey().indexOf(':') < 0) continue;
            NamespacedKey key = NamespacedKey.fromString(entry.getKey());
            if (key != null) container.values.put(key, fromNbtValue(entry.getValue()));
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

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonPersistentDataContainer container
            && Snbt.write(toNbt()).equals(Snbt.write(container.toNbt()));
    }

    @Override
    public int hashCode() {
        return Snbt.write(toNbt()).hashCode();
    }

    @Override
    public String toString() {
        return Snbt.write(toNbt());
    }
}
