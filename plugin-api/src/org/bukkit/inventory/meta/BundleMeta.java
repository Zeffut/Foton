package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.inventory.ItemStack;

/** Metadata carried by bundle items. */
public interface BundleMeta extends ItemMeta {
    List<ItemStack> getItems();
    default boolean hasItems() { return !getItems().isEmpty(); }

    void setItems(List<ItemStack> items);
    default void addItem(ItemStack item) { if (item == null) return; List<ItemStack> values = getItems(); values.add(item); setItems(values); }
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
