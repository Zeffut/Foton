package org.bukkit.entity;

import java.util.List;
import org.bukkit.potion.PotionEffect;

/** An arrow projectile, including effects carried by tipped/custom arrows. */
public interface Arrow extends AbstractArrow {
    /** Sets the native base potion; null clears the registered base potion. */
    void setBasePotionType(org.bukkit.potion.PotionType type);
    /** Returns the base potion when the arrow carries one; null for plain arrows. */
    org.bukkit.potion.PotionType getBasePotionType();
    default org.bukkit.potion.PotionData getBasePotionData() { return null; }
    default org.bukkit.Color getColor() { return null; }
    List<PotionEffect> getCustomEffects();
}
