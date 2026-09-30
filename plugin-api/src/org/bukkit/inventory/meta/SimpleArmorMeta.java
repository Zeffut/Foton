package org.bukkit.inventory.meta;

import org.bukkit.inventory.meta.trim.ArmorTrim;

/** The meta of a trimmable armor piece: its trim is the item's trim component. */
public final class SimpleArmorMeta extends SimpleItemMeta implements ArmorMeta {
    @Override public ArmorTrim getTrim() { return getTrimComponent(); }
    @Override public boolean hasTrim() { return getTrimComponent() != null; }
    @Override public void setTrim(ArmorTrim trim) { setTrimComponent(trim); }
    @Override public SimpleArmorMeta clone() { return (SimpleArmorMeta) super.clone(); }
}
