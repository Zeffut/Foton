package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.potion.PotionEffect;

public interface SuspiciousStewMeta extends ItemMeta {
    @Override SuspiciousStewMeta clone();
    boolean hasCustomEffects();
    List<PotionEffect> getCustomEffects();
    boolean addCustomEffect(PotionEffect effect, boolean overwrite);
    boolean removeCustomEffect(org.bukkit.potion.PotionEffectType type);
    boolean clearCustomEffects();
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
