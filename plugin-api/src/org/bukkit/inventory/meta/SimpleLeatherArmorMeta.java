package org.bukkit.inventory.meta;

import org.bukkit.Color;

public final class SimpleLeatherArmorMeta extends SimpleItemMeta implements LeatherArmorMeta {
    private Color color = Color.fromRGB(0xA06540);
    private org.bukkit.inventory.meta.trim.ArmorTrim trim;
    @Override public Color getColor() { return color; }
    @Override public void setColor(Color value) { color = value == null ? Color.fromRGB(0xA06540) : value; }
    @Override public org.bukkit.inventory.meta.trim.ArmorTrim getTrim() { return trim; }
    @Override public boolean hasTrim() { return trim != null; }
    @Override public void setTrim(org.bukkit.inventory.meta.trim.ArmorTrim value) { trim = value; }
    @Override public SimpleLeatherArmorMeta clone() { SimpleLeatherArmorMeta copy = (SimpleLeatherArmorMeta) super.clone(); copy.color = color; copy.trim = trim; return copy; }
}
