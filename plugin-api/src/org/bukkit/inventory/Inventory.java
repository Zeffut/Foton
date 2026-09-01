package org.bukkit.inventory;

import org.bukkit.Material;

/** Somewhere items are kept.
 *
 * Every read gives copies and every write takes them. That is Bukkit's
 * contract and it is also the only safe one across JNI: a plugin holding a
 * live reference into a chest could change it from its own thread while the
 * tick was reading it.
 */
public interface Inventory {
    InventoryHolder getHolder();
    int getSize();

    default boolean isEmpty() {
        for (int slot = 0; slot < getSize(); slot++) {
            ItemStack item = getItem(slot);
            if (item != null && !item.getType().isAir() && item.getAmount() > 0) return false;
        }
        return true;
    }

    ItemStack getItem(int slot);

    void setItem(int slot, ItemStack item);

    java.util.HashMap<Integer, ItemStack> addItem(ItemStack... items);

    ItemStack[] getContents();

    void setContents(ItemStack[] items);

    boolean contains(Material material);

    int first(Material material);

    void clear();

    void clear(int slot);
}
