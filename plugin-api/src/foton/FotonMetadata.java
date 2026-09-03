package foton;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.IdentityHashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import org.bukkit.metadata.MetadataValue;
import org.bukkit.plugin.Plugin;

/** Plugin-owned metadata attached to live entity handles. */
final class FotonMetadata {
    private static final ConcurrentHashMap<UUID, ConcurrentHashMap<String, List<MetadataValue>>> VALUES =
        new ConcurrentHashMap<>();
    private static final Map<Object, Map<String, List<MetadataValue>>> OBJECT_VALUES = new IdentityHashMap<>();

    static synchronized void set(Object owner, String key, MetadataValue value) {
        if (owner == null || key == null || value == null) return;
        Map<String, List<MetadataValue>> map = OBJECT_VALUES.computeIfAbsent(owner, ignored -> new java.util.HashMap<>());
        List<MetadataValue> current = map.get(key);
        ArrayList<MetadataValue> next = new ArrayList<>();
        if (current != null) for (MetadataValue entry : current) if (entry.getOwningPlugin() != value.getOwningPlugin()) next.add(entry);
        next.add(value);
        map.put(key, List.copyOf(next));
    }
    static void set(UUID id, String key, MetadataValue value) {
        if (id == null || key == null || value == null) return;
        VALUES.computeIfAbsent(id, ignored -> new ConcurrentHashMap<>())
            .compute(key, (ignored, current) -> {
                ArrayList<MetadataValue> next = new ArrayList<>();
                if (current != null) for (MetadataValue entry : current)
                    if (entry.getOwningPlugin() != value.getOwningPlugin()) next.add(entry);
                next.add(value);
                return List.copyOf(next);
            });
    }
    static synchronized List<MetadataValue> get(Object owner, String key) {
        if (owner == null || key == null) return List.of();
        Map<String, List<MetadataValue>> map = OBJECT_VALUES.get(owner);
        List<MetadataValue> values = map == null ? null : map.get(key);
        return values == null ? List.of() : values;
    }
    static List<MetadataValue> get(UUID id, String key) {
        List<MetadataValue> values = id == null || key == null ? null : VALUES.getOrDefault(id, new ConcurrentHashMap<>()).get(key);
        return values == null ? List.of() : values;
    }
    static synchronized void remove(Object owner, String key, Plugin plugin) {
        if (owner == null || key == null) return;
        Map<String, List<MetadataValue>> map = OBJECT_VALUES.get(owner);
        if (map == null) return;
        map.computeIfPresent(key, (ignored, current) -> current.stream().filter(value -> value.getOwningPlugin() != plugin).toList());
    }
    static void remove(UUID id, String key, Plugin plugin) {
        if (id == null || key == null) return;
        ConcurrentHashMap<String, List<MetadataValue>> map = VALUES.get(id);
        if (map == null) return;
        map.computeIfPresent(key, (ignored, current) -> current.stream()
            .filter(value -> value.getOwningPlugin() != plugin).toList());
    }
}
