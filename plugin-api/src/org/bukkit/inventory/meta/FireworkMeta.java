package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.FireworkEffect;

public interface FireworkMeta extends ItemMeta {
    int getPower();
    void setPower(int power);
    @Override FireworkMeta clone();
    default boolean hasPower() { return getPower() > 0; }
    List<FireworkEffect> getEffects();
    default boolean hasEffects() { return !getEffects().isEmpty(); }
    void addEffect(FireworkEffect effect);
    boolean removeEffect(int index);
    void clearEffects();
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
