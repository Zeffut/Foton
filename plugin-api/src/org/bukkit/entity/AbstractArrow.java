package org.bukkit.entity;

import java.util.List;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;
import org.jetbrains.annotations.Unmodifiable;

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
    @Nullable
    @Deprecated(since = "1.21.4")
    org.bukkit.block.Block getAttachedBlock();
    @NotNull
    @Unmodifiable
    List<org.bukkit.block.Block> getAttachedBlocks();
    @NotNull
    PickupStatus getPickupStatus();
    void setPickupStatus(@NotNull PickupStatus status);
    boolean isShotFromCrossbow();
    @Deprecated(since = "1.21", forRemoval = true)
    void setShotFromCrossbow(boolean shotFromCrossbow);
    @Deprecated(since = "1.20.4", forRemoval = true)
    @NotNull
    org.bukkit.inventory.ItemStack getItem();
    @Deprecated(since = "1.20.4", forRemoval = true)
    void setItem(@NotNull org.bukkit.inventory.ItemStack item);
    @Nullable
    org.bukkit.inventory.ItemStack getWeapon();
    void setWeapon(@NotNull org.bukkit.inventory.ItemStack item);

    enum PickupStatus { DISALLOWED, ALLOWED, CREATIVE_ONLY }

    @Deprecated
    default PickupRule getPickupRule() { return PickupRule.valueOf(getPickupStatus().name()); }
    @Deprecated
    default void setPickupRule(PickupRule rule) {
        setPickupStatus(PickupStatus.valueOf(rule.name()));
    }
    @Deprecated
    enum PickupRule { DISALLOWED, ALLOWED, CREATIVE_ONLY }

    @NotNull org.bukkit.inventory.ItemStack getItemStack();
    void setItemStack(@NotNull org.bukkit.inventory.ItemStack stack);
    void setLifetimeTicks(int ticks);
    int getLifetimeTicks();
    @NotNull org.bukkit.Sound getHitSound();
    void setHitSound(@NotNull org.bukkit.Sound sound);
    void setShooter(@Nullable org.bukkit.projectiles.ProjectileSource source, boolean resetPickupStatus);
}
