package org.bukkit.inventory.meta.trim;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;

/** Identifies the pattern used to render an armor trim. */
public final class TrimPattern implements Keyed {
    private final NamespacedKey key;
    public TrimPattern(NamespacedKey key) { this.key = key; }
    public TrimPattern(String key) { this(NamespacedKey.minecraft(key)); }
    @Override public NamespacedKey getKey() { return key; }
}
