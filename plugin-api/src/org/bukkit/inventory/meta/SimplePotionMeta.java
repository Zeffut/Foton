package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.potion.PotionData;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;

public final class SimplePotionMeta extends SimpleItemMeta implements PotionMeta {
    private List<PotionEffect> effects = new ArrayList<>();
    private PotionData base;
    private org.bukkit.Color color;
    @Override public List<PotionEffect> getCustomEffects() { return Collections.unmodifiableList(effects); }
    @Override public boolean addCustomEffect(PotionEffect effect, boolean overwrite) {
        if (effect == null) return false;
        for (int i = 0; i < effects.size(); i++) if (effects.get(i).getType().equals(effect.getType())) {
            if (!overwrite) return false; effects.set(i, effect); return true;
        }
        effects.add(effect); return true;
    }
    @Override public boolean removeCustomEffect(PotionEffectType type) { return effects.removeIf(e -> e.getType().equals(type)); }
    @Override public boolean hasCustomEffect(PotionEffectType type) { return effects.stream().anyMatch(e -> e.getType().equals(type)); }
    @Override public void setBasePotionData(PotionData data) { base = data; }
    @Override public PotionData getBasePotionData() { return base; }
    @Override public org.bukkit.Color getColor() { return color; }
    @Override public void setColor(org.bukkit.Color color) { this.color = color; }
        @Override public SimplePotionMeta clone() { SimplePotionMeta copy = (SimplePotionMeta) super.clone(); copy.effects = new ArrayList<>(effects); copy.base = base; copy.color = color; return copy; }
    @Override public boolean equals(Object other) {
        return other instanceof SimplePotionMeta meta && super.equals(other)
            && effects.equals(meta.effects) && java.util.Objects.equals(base, meta.base)
            && java.util.Objects.equals(color, meta.color);
    }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), effects, base, color); }
}
