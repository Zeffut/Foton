package org.bukkit.entity;

/** An entity with living characteristics. */
public interface LivingEntity extends Damageable, org.bukkit.attribute.Attributable {
    default org.bukkit.block.Block getTargetBlock(java.util.Set<org.bukkit.Material> transparent, int maxDistance) {
        if (maxDistance <= 0 || getLocation() == null || getWorld() == null) return null;
        org.bukkit.Location origin = getEyeLocation();
        if (origin == null) return null;
        org.bukkit.util.Vector direction = origin.getDirection().normalize();
        for (int step = 0; step <= maxDistance * 10; step++) {
            double distance = step / 10.0;
            org.bukkit.Location point = origin.clone().add(direction.clone().multiply(distance));
            org.bukkit.block.Block block = getWorld().getBlockAt(point.getBlockX(), point.getBlockY(), point.getBlockZ());
            org.bukkit.Material material = block.getType();
            if (!material.isAir() && (transparent == null || !transparent.contains(material))) return block;
        }
        return null;
    }

    default org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() { return null; }
    default void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent event) { }
    default boolean isHandRaised() { return false; }
    default void clearActiveItem() { }
    default org.bukkit.util.BoundingBox getBoundingBox() { return null; }
    default boolean isInvisible() { return false; }
    default boolean isCustomNameVisible() { return false; }
    default double getEyeHeight() { return 1.62; }
    default int getNoDamageTicks() { return 0; }
    default void setNoDamageTicks(int ticks) { }
    default int getFreezeTicks() { return 0; }
    default void setFreezeTicks(int ticks) { }
    default int getMaxFreezeTicks() { return 140; }
    default boolean isFrozen() { return getFreezeTicks() >= getMaxFreezeTicks(); }
    default java.util.List<Entity> getNearbyEntities(double x, double y, double z) { return java.util.List.of(); }
    default org.bukkit.attribute.AttributeInstance getAttribute(org.bukkit.attribute.Attribute attribute) { return null; }
    default int getAir() { return 300; }
    default void setAir(int ticks) { }
    /** Sets the remaining air supply. */
    default void setRemainingAir(int ticks) { setAir(ticks); }
    default float getFallDistance() { return 0.0f; }
    default void setFallDistance(float distance) { }
    default int getMaximumAir() { return 300; }
    default void setMaximumAir(int ticks) { }
    default boolean hasPotionEffect(org.bukkit.potion.PotionEffectType type) { return false; }
    default org.bukkit.potion.PotionEffect getPotionEffect(org.bukkit.potion.PotionEffectType type) { return null; }
    default java.util.Collection<org.bukkit.potion.PotionEffect> getActivePotionEffects() { return java.util.List.of(); }
    default boolean addPotionEffect(org.bukkit.potion.PotionEffect effect) { return false; }
    default void removePotionEffect(org.bukkit.potion.PotionEffectType type) { }
    default org.bukkit.inventory.EntityEquipment getEquipment() { return null; }
    default boolean isPersistent() { return true; }
    default void setPersistent(boolean persistent) { }
    default boolean getRemoveWhenFarAway() { return false; }
    default void setRemoveWhenFarAway(boolean remove) { }
    default boolean getCanPickupItems() { return true; }
    default void setCanPickupItems(boolean pickup) { }
}
