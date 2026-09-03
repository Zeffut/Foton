package org.bukkit.inventory.meta;

import org.bukkit.inventory.meta.trim.ArmorTrim;

public interface ArmorMeta extends ItemMeta {
    ArmorTrim getTrim();
    boolean hasTrim();
    void setTrim(ArmorTrim trim);
}
