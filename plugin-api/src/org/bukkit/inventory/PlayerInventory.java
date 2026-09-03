package org.bukkit.inventory;

import org.bukkit.Material;

/** A player's own inventory, with the slots that have names. */
public interface PlayerInventory extends Inventory {
    default java.util.HashMap<Integer, ItemStack> all(Material material) {
        java.util.HashMap<Integer, ItemStack> result = new java.util.HashMap<>();
        if (material == null) return result;
        for (int slot = 0; slot < getSize(); slot++) { ItemStack item = getItem(slot); if (item != null && item.getType() == material) result.put(slot, item); }
        return result;
    }

    default ItemStack getItem(EquipmentSlot slot) {
        if (slot == null) return null;
        return switch (slot) { case HAND -> getItemInMainHand(); case OFF_HAND -> getItemInOffHand(); case HEAD -> getHelmet(); case CHEST -> getChestplate(); case LEGS -> getLeggings(); case FEET -> getBoots(); default -> null; };
    }
    default java.util.Spliterator<ItemStack> spliterator() {
        return java.util.Arrays.spliterator(getContents());
    }

    default void setItem(EquipmentSlot slot, ItemStack item) {
        if (slot == null) return;
        switch (slot) {
            case HAND: setItemInMainHand(item); break;
            case OFF_HAND: setItemInOffHand(item); break;
            case HEAD: setHelmet(item); break;
            case CHEST: setChestplate(item); break;
            case LEGS: setLeggings(item); break;
            case FEET: setBoots(item); break;
            default: break;
        }
    }
    ItemStack getItemInMainHand();

    void setItemInMainHand(ItemStack item);

    ItemStack getItemInHand();

    void setItemInHand(ItemStack item);

    ItemStack getItemInOffHand();

    void setItemInOffHand(ItemStack item);

    ItemStack getHelmet();
    void setHelmet(ItemStack item);

    ItemStack getChestplate();
    void setChestplate(ItemStack item);

    ItemStack getLeggings();
    void setLeggings(ItemStack item);

    ItemStack getBoots();
    void setBoots(ItemStack item);

    default ItemStack[] getArmorContents() {
        return new ItemStack[] { getBoots(), getLeggings(), getChestplate(), getHelmet() };
    }

    default void setArmorContents(ItemStack[] contents) {
        setBoots(contents != null && contents.length > 0 ? contents[0] : null);
        setLeggings(contents != null && contents.length > 1 ? contents[1] : null);
        setChestplate(contents != null && contents.length > 2 ? contents[2] : null);
        setHelmet(contents != null && contents.length > 3 ? contents[3] : null);
    }

    /** The 36 main-inventory and hotbar slots, excluding armor and offhand. */
    default ItemStack[] getStorageContents() {
        ItemStack[] result = new ItemStack[36];
        for (int slot = 0; slot < result.length; slot++) result[slot] = getItem(slot);
        return result;
    }

    default void setStorageContents(ItemStack[] contents) {
        for (int slot = 0; slot < 36; slot++) setItem(slot, contents != null && slot < contents.length ? contents[slot] : null);
    }

    default int firstEmpty() {
        for (int slot = 0; slot < 36; slot++) {
            ItemStack item = getItem(slot);
            if (item == null || item.getType() == Material.AIR || item.getAmount() <= 0) return slot;
        }
        return -1;
    }

    int getHeldItemSlot();
    default void setHeldItemSlot(int slot) { }
}
