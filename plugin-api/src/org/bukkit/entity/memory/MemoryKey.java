package org.bukkit.entity.memory;
import org.bukkit.NamespacedKey;
/** Typed key identifying a vanilla entity memory module. */
public final class MemoryKey<T> {
    public static final MemoryKey<org.bukkit.Location> MEETING_POINT = new MemoryKey<>("meeting_point", org.bukkit.Location.class);
    public static final MemoryKey<org.bukkit.Location> HOME = new MemoryKey<>("home", org.bukkit.Location.class);
    public static final MemoryKey<org.bukkit.Location> POTENTIAL_JOB_SITE = new MemoryKey<>("potential_job_site", org.bukkit.Location.class);
    public static final MemoryKey<org.bukkit.Location> JOB_SITE = new MemoryKey<>("job_site", org.bukkit.Location.class);
    public static final MemoryKey<org.bukkit.Location> LAST_WORKED_AT_POI = new MemoryKey<>("last_worked_at_poi", org.bukkit.Location.class);
    public static final MemoryKey<org.bukkit.entity.Entity> ATTACK_TARGET = new MemoryKey<>("attack_target", org.bukkit.entity.Entity.class);
    private final NamespacedKey key; private final Class<T> memoryClass;
    private MemoryKey(String name, Class<T> memoryClass) { this.key = NamespacedKey.minecraft(name); this.memoryClass = memoryClass; }
    public NamespacedKey getKey() { return key; }
    public Class<T> getMemoryClass() { return memoryClass; }
    public static MemoryKey<?> getByKey(NamespacedKey key) {
        if (key == null || !"minecraft".equals(key.getNamespace())) return null;
        for (MemoryKey<?> value : new MemoryKey<?>[]{HOME, POTENTIAL_JOB_SITE, JOB_SITE, LAST_WORKED_AT_POI, MEETING_POINT, ATTACK_TARGET})
            if (value.key.equals(key)) return value;
        return null;
    }
}
