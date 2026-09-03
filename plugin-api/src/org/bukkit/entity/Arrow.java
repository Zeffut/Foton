package org.bukkit.entity;

import java.util.List;
import org.bukkit.potion.PotionEffect;

/** An arrow projectile, including effects carried by tipped/custom arrows. */
public interface Arrow extends AbstractArrow {
    /** Returns the base potion when the arrow carries one; null for plain arrows. */
    default org.bukkit.potion.PotionType getBasePotionType() { return null; }
    default org.bukkit.potion.PotionData getBasePotionData() { return null; }
    default org.bukkit.Color getColor() { return null; }
    List<PotionEffect> getCustomEffects();
}
