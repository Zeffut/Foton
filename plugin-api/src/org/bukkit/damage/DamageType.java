package org.bukkit.damage;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;

public final class DamageType implements Keyed {
    public static final DamageType MAGIC = new DamageType("magic");
    private final NamespacedKey key;
    public DamageType(String key) { this.key = NamespacedKey.minecraft(key); }
    @Override public NamespacedKey getKey() { return key; }
}
