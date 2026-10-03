package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.FireworkEffect;

public final class SimpleFireworkMeta extends SimpleItemMeta implements FireworkMeta {
    private List<FireworkEffect> effects = new ArrayList<>();
    private int power;
    @Override public int getPower() { return power; }
    @Override public void setPower(int power) { this.power = Math.max(0, Math.min(3, power)); unsupportedChange("fireworks"); }
    @Override public List<FireworkEffect> getEffects() { return List.copyOf(effects); }
    @Override public void addEffect(FireworkEffect effect) { if (effect != null) { effects.add(effect); unsupportedChange("fireworks"); } }
    @Override public boolean removeEffect(int index) { if (nativeState().hasNativeBase()) unsupportedChange("fireworks"); if (index < 0 || index >= effects.size()) return false; effects.remove(index); unsupportedChange("fireworks"); return true; }
    @Override public void clearEffects() { if (!effects.isEmpty() || nativeState().hasNativeBase()) { effects.clear(); unsupportedChange("fireworks"); } }
    @Override public SimpleFireworkMeta clone() { SimpleFireworkMeta copy = (SimpleFireworkMeta) super.clone(); copy.effects = new ArrayList<>(effects); copy.power = power; return copy; }
    @Override public boolean equals(Object other) { return other instanceof SimpleFireworkMeta meta && super.equals(other) && power == meta.power && effects.equals(meta.effects); }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), effects, power); }
}
