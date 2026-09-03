package org.bukkit.inventory.meta;

import org.bukkit.Color;

public interface LeatherArmorMeta extends ArmorMeta {
    Color getColor();
    void setColor(Color color);
    @Override LeatherArmorMeta clone();
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
