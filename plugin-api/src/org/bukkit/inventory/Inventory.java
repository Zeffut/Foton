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
    default java.util.List<org.bukkit.entity.HumanEntity> getViewers() { return java.util.Collections.emptyList(); }
    default void forEach(java.util.function.Consumer<? super ItemStack> action) {
        if (action == null) return;
        for (ItemStack item : getContents()) action.accept(item);
    }

    /** Iterates over a snapshot of the inventory contents. */
    default java.util.ListIterator<ItemStack> iterator() {
        return java.util.Arrays.asList(getContents()).listIterator();
    }
    default org.bukkit.event.inventory.InventoryType getType() { return org.bukkit.event.inventory.InventoryType.UNKNOWN; }
    InventoryHolder getHolder();
    default org.bukkit.Location getLocation() { return null; }

    /** Returns the holder, optionally allowing a null result for snapshots. */
    default InventoryHolder getHolder(boolean useSnapshot) { return getHolder(); }
    default int getMaxStackSize() { return 64; }

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
    default java.util.HashMap<Integer, ItemStack> removeItem(ItemStack... items) {
        java.util.HashMap<Integer, ItemStack> leftover = new java.util.HashMap<>();
        if (items == null) return leftover;
        for (int input = 0; input < items.length; input++) {
            ItemStack wanted = items[input];
            if (wanted == null || wanted.getType().isAir()) continue;
            int remaining = wanted.getAmount();
            for (int slot = 0; slot < getSize() && remaining > 0; slot++) {
                ItemStack current = getItem(slot);
                if (current == null || !current.isSimilar(wanted)) continue;
                int removed = Math.min(remaining, current.getAmount());
                current.setAmount(current.getAmount() - removed);
                setItem(slot, current.getAmount() <= 0 ? null : current);
                remaining -= removed;
            }
            if (remaining > 0) { ItemStack rest = wanted.clone(); rest.setAmount(remaining); leftover.put(input, rest); }
        }
        return leftover;
    }

    ItemStack[] getContents();

    /** Returns the slots that can hold ordinary storage items. */
    default ItemStack[] getStorageContents() { return getContents(); }
    default void setStorageContents(ItemStack[] items) { setContents(items); }

    void setContents(ItemStack[] items);

    boolean contains(Material material);

    default boolean contains(Material material, int amount) {
        if (material == null || amount <= 0) return amount <= 0;
        int total = 0;
        for (ItemStack item : getContents()) if (item != null && item.getType() == material) {
            total += item.getAmount();
            if (total >= amount) return true;
        }
        return false;
    }

    int first(Material material);

    void clear();

    void clear(int slot);
}
