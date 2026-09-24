package org.bukkit.entity;

/** An entity with living characteristics. */
public interface LivingEntity extends Damageable, org.bukkit.attribute.Attributable, org.bukkit.projectiles.ProjectileSource {
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

    /** Whether the entity thinks: always false for anything that is not a mob. */
    default boolean hasAI() { return foton.Native.entityHasAi(getUniqueId().toString()); }
    /** Vanilla's NoAI switch: a mob without AI does not move at all. */
    default void setAI(boolean ai) { foton.Native.setEntityAi(getUniqueId().toString(), ai); }
    default boolean isCollidable() { return foton.Native.entityCollidable(getUniqueId().toString()); }
    /** Whether other entities push this one, and it them. */
    default void setCollidable(boolean collidable) { foton.Native.setEntityCollidable(getUniqueId().toString(), collidable); }
    default org.bukkit.event.entity.EntityDamageEvent getLastDamageCause() { return null; }
    default void setLastDamageCause(org.bukkit.event.entity.EntityDamageEvent event) { }
    default boolean isHandRaised() { return false; }
    default void clearActiveItem() { }
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

    /** The item being used -- a drawn bow, a raised shield, food being eaten -- or air. */
    default org.bukkit.inventory.ItemStack getActiveItem() {
        org.bukkit.inventory.ItemStack item = foton.FotonInventory.decode(foton.Native.activeItem(getUniqueId().toString()));
        return item == null ? new org.bukkit.inventory.ItemStack(org.bukkit.Material.AIR) : item;
    }
    /** Whether the entity is on a ladder, vine or other climbable block. */
    default boolean isClimbing() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & 1) != 0; }
    /** Whether the entity is in a riptide spin attack. */
    default boolean isRiptiding() { return (foton.Native.entitySurroundings(getUniqueId().toString()) & 8) != 0; }
    /** Vanilla's fall-flying flag; the server clears it again when the entity cannot glide. */
    default void setGliding(boolean gliding) { foton.Native.setEntityGliding(getUniqueId().toString(), gliding); }
    /** Vanilla's swimming flag; a player's is recomputed every tick from where they are. */
    default void setSwimming(boolean swimming) { foton.Native.setEntitySwimming(getUniqueId().toString(), swimming); }
}
