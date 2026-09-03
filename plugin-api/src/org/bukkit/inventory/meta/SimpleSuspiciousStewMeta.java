package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;

public final class SimpleSuspiciousStewMeta extends SimpleItemMeta implements SuspiciousStewMeta {
    private List<PotionEffect> effects = new ArrayList<>();
    @Override public boolean hasCustomEffects() { return !effects.isEmpty(); }
    @Override public List<PotionEffect> getCustomEffects() { return Collections.unmodifiableList(effects); }
    @Override public boolean addCustomEffect(PotionEffect effect, boolean overwrite) {
        if (effect == null) return false;
        if (overwrite) removeCustomEffect(effect.getType());
        else for (PotionEffect current : effects) if (current.getType() == effect.getType()) return false;
        effects.add(effect); return true;
    }
    @Override public boolean removeCustomEffect(PotionEffectType type) { return effects.removeIf(effect -> effect.getType() == type); }
    @Override public boolean clearCustomEffects() { boolean had = !effects.isEmpty(); effects.clear(); return had; }
    @Override public SimpleSuspiciousStewMeta clone() { SimpleSuspiciousStewMeta copy = (SimpleSuspiciousStewMeta) super.clone(); copy.effects = new ArrayList<>(effects); return copy; }
    @Override public boolean equals(Object other) { return other instanceof SimpleSuspiciousStewMeta meta && super.equals(other) && effects.equals(meta.effects); }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), effects); }
}
