package org.bukkit.entity;

import org.bukkit.inventory.meta.FireworkMeta;

/** A launched firework rocket. */
public interface Firework extends Entity {
    default void setFireworkMeta(FireworkMeta meta) { }
    default FireworkMeta getFireworkMeta() { return new org.bukkit.inventory.meta.SimpleFireworkMeta(); }
    default java.util.UUID getSpawningEntity() {
        if (!(this instanceof foton.FotonEntity entity)) return null;
        String owner = foton.Native.entityProjectileOwner(entity.getUniqueId().toString());
        try { return owner == null ? null : java.util.UUID.fromString(owner); }
        catch (IllegalArgumentException ignored) { return null; }
    }
}
