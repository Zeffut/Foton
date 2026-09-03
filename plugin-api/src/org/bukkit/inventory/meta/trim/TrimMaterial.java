package org.bukkit.inventory.meta.trim;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;

/** Identifies the material used to render an armor trim. */
public final class TrimMaterial implements Keyed {
    private final NamespacedKey key;
    public TrimMaterial(NamespacedKey key) { this.key = key; }
    public TrimMaterial(String key) { this(NamespacedKey.minecraft(key)); }
    @Override public NamespacedKey getKey() { return key; }
}
