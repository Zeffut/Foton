package foton;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import org.bukkit.NamespacedKey;
import org.bukkit.persistence.PersistentDataContainer;
import org.bukkit.persistence.PersistentDataType;

/** In-memory typed container; persistence is carried by the owning meta object. */
public final class FotonPersistentDataContainer implements PersistentDataContainer {
    private final Map<NamespacedKey, Object> values = new HashMap<>();
    @Override public <P, C> void set(NamespacedKey key, PersistentDataType<P, C> type, C value) {
        if (key == null || type == null) throw new IllegalArgumentException("key and type are required");
        if (value == null) { values.remove(key); return; }
        values.put(key, value);
    }
    @SuppressWarnings("unchecked")
    @Override public <P, C> C get(NamespacedKey key, PersistentDataType<P, C> type) {
        return (C) values.get(key);
    }
    @Override public <P, C> C getOrDefault(NamespacedKey key, PersistentDataType<P, C> type, C fallback) {
        C value = get(key, type);
        return value == null ? fallback : value;
    }
    @Override public <P, C> boolean has(NamespacedKey key, PersistentDataType<P, C> type) {
        return values.containsKey(key);
    }
    @Override public void remove(NamespacedKey key) { values.remove(key); }
    @Override public Set<NamespacedKey> getKeys() { return new HashSet<>(values.keySet()); }
    public FotonPersistentDataContainer copy() {
        FotonPersistentDataContainer copy = new FotonPersistentDataContainer();
        copy.values.putAll(values);
        return copy;
    }
    @Override public boolean equals(Object other) {
        return other instanceof FotonPersistentDataContainer container && values.equals(container.values);
    }
    @Override public int hashCode() { return values.hashCode(); }
}
