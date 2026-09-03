package org.bukkit.inventory;

/** Equipment slots exposed for a living entity. */
public interface EntityEquipment {
    default org.bukkit.entity.Entity getHolder() { return null; }
    ItemStack[] getArmorContents();
    default ItemStack getHelmet() { ItemStack[] armor = getArmorContents(); return armor.length > 3 ? armor[3] : null; }
    default void setHelmet(ItemStack item) { ItemStack[] armor = getArmorContents(); if (armor.length > 3) { armor[3] = item; setArmorContents(armor); } }
    default ItemStack getChestplate() { ItemStack[] armor = getArmorContents(); return armor.length > 2 ? armor[2] : null; }
    default void setChestplate(ItemStack item) { ItemStack[] armor = getArmorContents(); if (armor.length > 2) { armor[2] = item; setArmorContents(armor); } }
    default ItemStack getBoots() { return getArmorContents().length > 0 ? getArmorContents()[0] : null; }
    default void setBoots(ItemStack item) { ItemStack[] armor = getArmorContents(); if (armor.length > 0) { armor[0] = item; setArmorContents(armor); } }
    default ItemStack getLeggings() { ItemStack[] armor = getArmorContents(); return armor.length > 2 ? armor[2] : null; }
    default void setLeggings(ItemStack item) { ItemStack[] armor = getArmorContents(); if (armor.length > 2) { armor[2] = item; setArmorContents(armor); } }
    default void setArmorContents(ItemStack[] items) { }
    ItemStack getItemInMainHand();
    void setItemInMainHand(ItemStack item);
    /** @deprecated use main-hand methods. */
    @Deprecated
    default ItemStack getItemInHand() { return getItemInMainHand(); }
    /** @deprecated use main-hand methods. */
    @Deprecated
    default void setItemInHand(ItemStack item) { setItemInMainHand(item); }
    ItemStack getItemInOffHand();
    void setItemInOffHand(ItemStack item);
    default float getItemInHandDropChance() { return 0.085f; }
    default void setItemInHandDropChance(float chance) { }
    default float getItemInMainHandDropChance() { return getItemInHandDropChance(); }
    default void setItemInMainHandDropChance(float chance) { setItemInHandDropChance(chance); }
    default float getHelmetDropChance() { return 0.085f; }
    default void setHelmetDropChance(float chance) { }
    default float getChestplateDropChance() { return 0.085f; }
    default void setChestplateDropChance(float chance) { }
    default float getLeggingsDropChance() { return 0.085f; }
    default void setLeggingsDropChance(float chance) { }
    default float getBootsDropChance() { return 0.085f; }
    default void setBootsDropChance(float chance) { }
    /** Clears all armor and hand slots. */
    void clear();
}
