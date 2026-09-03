package org.bukkit.inventory.meta;

import org.bukkit.FireworkEffect;

public interface FireworkEffectMeta extends ItemMeta {
    boolean hasEffect();
    FireworkEffect getEffect();
    void setEffect(FireworkEffect effect);
    @Override FireworkEffectMeta clone();

    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
