package org.bukkit.entity;

import org.bukkit.inventory.meta.FireworkMeta;

/** A launched firework rocket. */
public interface Firework extends Projectile {
    /** The living entity this rocket is boosting, such as a gliding player, or null. */
    default LivingEntity getAttachedTo() {
        String attached = foton.Native.fireworkAttachedTo(getUniqueId().toString());
        if (attached == null) return null;
        try {
            return foton.FotonEntity.of(java.util.UUID.fromString(attached)) instanceof LivingEntity living ? living : null;
        } catch (IllegalArgumentException ignored) { return null; }
    }
    default void setFireworkMeta(FireworkMeta meta) { }
    default FireworkMeta getFireworkMeta() { return new org.bukkit.inventory.meta.SimpleFireworkMeta(); }
    default java.util.UUID getSpawningEntity() {
        if (!(this instanceof foton.FotonEntity entity)) return null;
        String owner = foton.Native.entityProjectileOwner(entity.getUniqueId().toString());
        try { return owner == null ? null : java.util.UUID.fromString(owner); }
        catch (IllegalArgumentException ignored) { return null; }
    }
}
