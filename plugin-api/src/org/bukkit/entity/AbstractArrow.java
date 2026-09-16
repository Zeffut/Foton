package org.bukkit.entity;

/** Shared projectile state for arrows. */
public interface AbstractArrow extends Projectile {
    @Deprecated(since = "1.21", forRemoval = true)
    int getKnockbackStrength();
    @Deprecated(since = "1.21", forRemoval = true)
    void setKnockbackStrength(int knockbackStrength);
    double getDamage();
    void setDamage(double damage);
    int getPierceLevel();
    void setPierceLevel(int pierceLevel);
    boolean isCritical();
    void setCritical(boolean critical);
    boolean isInBlock();
    @Deprecated(since = "1.21.4")
    org.bukkit.block.Block getAttachedBlock();
    java.util.List<org.bukkit.block.Block> getAttachedBlocks();
    PickupStatus getPickupStatus();
    void setPickupStatus(PickupStatus status);
    boolean isShotFromCrossbow();
    @Deprecated(since = "1.21", forRemoval = true)
    void setShotFromCrossbow(boolean shotFromCrossbow);
    @Deprecated(since = "1.20.4", forRemoval = true)
    org.bukkit.inventory.ItemStack getItem();
    @Deprecated(since = "1.20.4", forRemoval = true)
    void setItem(org.bukkit.inventory.ItemStack item);
    org.bukkit.inventory.ItemStack getWeapon();
    void setWeapon(org.bukkit.inventory.ItemStack item);

    enum PickupStatus { DISALLOWED, ALLOWED, CREATIVE_ONLY }

    @Deprecated
    default PickupRule getPickupRule() { return PickupRule.valueOf(getPickupStatus().name()); }
    @Deprecated
    default void setPickupRule(PickupRule rule) {
        setPickupStatus(PickupStatus.valueOf(rule.name()));
    }
    @Deprecated
    enum PickupRule { DISALLOWED, ALLOWED, CREATIVE_ONLY }

    org.bukkit.inventory.ItemStack getItemStack();
    void setItemStack(org.bukkit.inventory.ItemStack stack);
    void setLifetimeTicks(int ticks);
    int getLifetimeTicks();
    org.bukkit.Sound getHitSound();
    void setHitSound(org.bukkit.Sound sound);
    void setShooter(org.bukkit.projectiles.ProjectileSource source, boolean resetPickupStatus);
}
