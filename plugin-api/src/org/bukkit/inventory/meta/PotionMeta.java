package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionData;
import org.bukkit.potion.PotionType;

public interface PotionMeta extends ItemMeta {
    @Override PotionMeta clone();
    default boolean hasColor() { return getColor() != null; }
    org.bukkit.Color getColor();
    void setColor(org.bukkit.Color color);
    List<PotionEffect> getCustomEffects();
    /** Returns whether this potion carries at least one custom effect. */
    default boolean hasCustomEffects() {
        return !getCustomEffects().isEmpty();
    }
    default boolean clearCustomEffects() {
        boolean hadEffects = hasCustomEffects();
        for (PotionEffect effect : new java.util.ArrayList<>(getCustomEffects())) {
            removeCustomEffect(effect.getType());
        }
        return hadEffects;
    }
    boolean addCustomEffect(PotionEffect effect, boolean overwrite);
    boolean removeCustomEffect(org.bukkit.potion.PotionEffectType type);
    boolean hasCustomEffect(org.bukkit.potion.PotionEffectType type);
    void setBasePotionData(PotionData data);
    PotionData getBasePotionData();

    /** Modern Bukkit spelling for the base potion type. */
    default void setBasePotionType(PotionType type) {
        setBasePotionData(type == null ? null : new PotionData(type));
    }

    /** Modern Bukkit spelling for the base potion type. */
    default PotionType getBasePotionType() {
        PotionData data = getBasePotionData();
        return data == null ? null : data.getType();
    }
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
