package org.bukkit.entity;

import java.util.List;
import org.bukkit.potion.PotionEffect;

/** An arrow projectile, including effects carried by tipped/custom arrows. */
public interface Arrow extends AbstractArrow {
    @Deprecated(since = "1.20.6", forRemoval = true)
    void setBasePotionData(org.bukkit.potion.PotionData data);
    @Deprecated(since = "1.20.6", forRemoval = true)
    org.bukkit.potion.PotionData getBasePotionData();
    void setBasePotionType(org.bukkit.potion.PotionType type);
    org.bukkit.potion.PotionType getBasePotionType();
    org.bukkit.Color getColor();
    void setColor(org.bukkit.Color color);
    boolean hasCustomEffects();
    List<PotionEffect> getCustomEffects();
    boolean addCustomEffect(PotionEffect effect, boolean overwrite);
    boolean removeCustomEffect(org.bukkit.potion.PotionEffectType type);
    boolean hasCustomEffect(org.bukkit.potion.PotionEffectType type);
    void clearCustomEffects();
}
