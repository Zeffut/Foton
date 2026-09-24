package org.bukkit.inventory.meta;

import org.bukkit.Color;

public final class SimpleLeatherArmorMeta extends SimpleItemMeta implements LeatherArmorMeta {
    private Color color = Color.fromRGB(0xA06540);
    @Override public Color getColor() { return color; }
    @Override public void setColor(Color value) { color = value == null ? Color.fromRGB(0xA06540) : value; }
    @Override public org.bukkit.inventory.meta.trim.ArmorTrim getTrim() { return getTrimComponent(); }
    @Override public boolean hasTrim() { return getTrimComponent() != null; }
    @Override public void setTrim(org.bukkit.inventory.meta.trim.ArmorTrim value) { setTrimComponent(value); }
    @Override public SimpleLeatherArmorMeta clone() { return (SimpleLeatherArmorMeta) super.clone(); }
}
