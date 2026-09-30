package org.bukkit.entity;

import java.util.List;
import org.bukkit.potion.PotionEffect;
import org.jetbrains.annotations.NotNull;
import org.jetbrains.annotations.Nullable;

/** An arrow projectile, including effects carried by tipped/custom arrows. */
public interface Arrow extends AbstractArrow {
    @Deprecated(since = "1.20.6", forRemoval = true)
    void setBasePotionData(@Nullable org.bukkit.potion.PotionData data);
    @Nullable
    @Deprecated(since = "1.20.6", forRemoval = true)
    org.bukkit.potion.PotionData getBasePotionData();
    void setBasePotionType(@Nullable org.bukkit.potion.PotionType type);
    @Nullable
    org.bukkit.potion.PotionType getBasePotionType();
    @Nullable
    org.bukkit.Color getColor();
    void setColor(@Nullable org.bukkit.Color color);
    boolean hasCustomEffects();
    @NotNull
    List<PotionEffect> getCustomEffects();
    boolean addCustomEffect(@NotNull PotionEffect effect, boolean overwrite);
    boolean removeCustomEffect(@NotNull org.bukkit.potion.PotionEffectType type);
    boolean hasCustomEffect(@Nullable org.bukkit.potion.PotionEffectType type);
    void clearCustomEffects();
}
