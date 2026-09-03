package org.bukkit.inventory.meta;

import org.bukkit.FireworkEffect;

public final class SimpleFireworkEffectMeta extends SimpleItemMeta implements FireworkEffectMeta {
    private FireworkEffect effect;
    @Override public boolean hasEffect() { return effect != null; }
    @Override public FireworkEffect getEffect() { return effect; }
    @Override public void setEffect(FireworkEffect value) { effect = value; }
    @Override public SimpleFireworkEffectMeta clone() { SimpleFireworkEffectMeta copy = (SimpleFireworkEffectMeta) super.clone(); copy.effect = effect; return copy; }
}
