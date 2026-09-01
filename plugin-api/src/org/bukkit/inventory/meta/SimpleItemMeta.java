package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;

/** The meta an ItemStack carries when nothing more specific is needed. */
public class SimpleItemMeta implements ItemMeta {
    private foton.FotonPersistentDataContainer persistentData = new foton.FotonPersistentDataContainer();
    private String displayName;
    private List<String> lore;
    private Integer customModelData;
    private boolean unbreakable;
    private java.util.Map<org.bukkit.enchantments.Enchantment, Integer> enchantments = new java.util.HashMap<>();

    @Override
    public boolean hasDisplayName() {
        return displayName != null && !displayName.isEmpty();
    }

    @Override
    public String getDisplayName() {
        return displayName == null ? "" : displayName;
    }

    @Override
    public void setDisplayName(String name) {
        this.displayName = name;
    }

    @Override
    public boolean hasLore() {
        return lore != null && !lore.isEmpty();
    }

    /** A copy, because Bukkit's is: a plugin that mutated the returned list
     * and expected the item to change would be surprised either way, and
     * matching the surprise is the compatible choice. */
    @Override
    public List<String> getLore() {
        return lore == null ? null : new ArrayList<>(lore);
    }

    @Override
    public void setLore(List<String> lore) {
        this.lore = lore == null ? null : new ArrayList<>(lore);
    }

    @Override
    public boolean hasCustomModelData() {
        return customModelData != null;
    }

    @Override
    public int getCustomModelData() {
        if (customModelData == null) {
            throw new IllegalStateException("no custom model data; check hasCustomModelData");
        }
        return customModelData;
    }

    @Override
    public void setCustomModelData(Integer data) {
        this.customModelData = data;
    }

    @Override
    public boolean isUnbreakable() {
        return unbreakable;
    }

    @Override
    public void setUnbreakable(boolean unbreakable) {
        this.unbreakable = unbreakable;
    }

    @Override
    public int getEnchantLevel(org.bukkit.enchantments.Enchantment enchantment) {
        return enchantments.getOrDefault(enchantment, 0);
    }

    @Override
    public boolean addEnchant(org.bukkit.enchantments.Enchantment enchantment, int level, boolean ignoreLevelRestriction) {
        if (enchantment == null || level <= 0) return false;
        Integer previous = enchantments.put(enchantment, level);
        return previous == null || previous != level;
    }

    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return persistentData;
    }

    @Override
    public SimpleItemMeta clone() {
        try {
            SimpleItemMeta copy = (SimpleItemMeta) super.clone();
            copy.lore = lore == null ? null : new ArrayList<>(lore);
            copy.persistentData = persistentData.copy();
            copy.enchantments = new java.util.HashMap<>(enchantments);
            return copy;
        } catch (CloneNotSupportedException impossible) {
            throw new AssertionError(impossible);
        }
    }

    @Override
    public boolean equals(Object other) {
        if (other == null || getClass() != other.getClass()) {
            return false;
        }
        SimpleItemMeta meta = (SimpleItemMeta) other;
        return java.util.Objects.equals(displayName, meta.displayName)
            && java.util.Objects.equals(lore, meta.lore)
            && java.util.Objects.equals(customModelData, meta.customModelData)
            && java.util.Objects.equals(persistentData, meta.persistentData)
            && unbreakable == meta.unbreakable
            && java.util.Objects.equals(enchantments, meta.enchantments);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(displayName, lore, customModelData, unbreakable, persistentData);
    }
}
