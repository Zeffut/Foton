package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;

/** The meta an ItemStack carries when nothing more specific is needed. */
public class SimpleItemMeta implements Damageable {
    private foton.FotonPersistentDataContainer persistentData = new foton.FotonPersistentDataContainer();
    private String displayName;
    private List<String> lore;
    private Integer customModelData;
    private boolean unbreakable;
    private int damage;
    private org.bukkit.inventory.meta.components.CustomModelDataComponent customModelDataComponent = new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent();
    private org.bukkit.NamespacedKey itemModel;
    private org.bukkit.NamespacedKey tooltipStyle;
    private boolean hideTooltip;

    @Override public int getDamage() { return damage; }
    @Override public void setDamage(int value) { damage = Math.max(0, value); }
    private java.util.Map<org.bukkit.enchantments.Enchantment, Integer> enchantments = new java.util.HashMap<>();
    private java.util.Set<org.bukkit.inventory.ItemFlag> itemFlags = new java.util.HashSet<>();
    private java.util.Map<org.bukkit.attribute.Attribute, java.util.List<org.bukkit.attribute.AttributeModifier>> attributes = new java.util.HashMap<>();

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

    @Override public org.bukkit.inventory.meta.components.CustomModelDataComponent getCustomModelDataComponent() { return customModelDataComponent.clone(); }
    @Override public void setCustomModelDataComponent(org.bukkit.inventory.meta.components.CustomModelDataComponent component) { customModelDataComponent = component == null ? new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent() : component.clone(); }
    @Override public boolean hasItemModel() { return itemModel != null; }
    @Override public org.bukkit.NamespacedKey getItemModel() { return itemModel; }
    @Override public void setItemModel(org.bukkit.NamespacedKey key) { itemModel = key; }
    @Override public boolean hasTooltipStyle() { return tooltipStyle != null; }
    @Override public org.bukkit.NamespacedKey getTooltipStyle() { return tooltipStyle; }
    @Override public void setTooltipStyle(org.bukkit.NamespacedKey key) { tooltipStyle = key; }
    @Override public boolean isHideTooltip() { return hideTooltip; }
    @Override public void setHideTooltip(boolean hide) { hideTooltip = hide; }

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
    public boolean removeEnchant(org.bukkit.enchantments.Enchantment enchantment) {
        return enchantments.remove(enchantment) != null;
    }

    @Override
    public int getEnchantLevel(org.bukkit.enchantments.Enchantment enchantment) {
        return enchantments.getOrDefault(enchantment, 0);
    }

    @Override
    public java.util.Map<org.bukkit.enchantments.Enchantment, Integer> getEnchants() {
        return java.util.Collections.unmodifiableMap(new java.util.HashMap<>(enchantments));
    }

    @Override
    public boolean addEnchant(org.bukkit.enchantments.Enchantment enchantment, int level, boolean ignoreLevelRestriction) {
        if (enchantment == null || level <= 0) return false;
        if (!ignoreLevelRestriction && level > enchantment.getMaxLevel()) return false;
        Integer previous = enchantments.put(enchantment, level);
        return previous == null || previous != level;
    }

    @Override public void addItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null) itemFlags.add(flag);
    }

    @Override public void removeItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null) itemFlags.remove(flag);
    }

    @Override public boolean hasItemFlag(org.bukkit.inventory.ItemFlag flag) { return itemFlags.contains(flag); }
    @Override public java.util.Set<org.bukkit.inventory.ItemFlag> getItemFlags() { return java.util.Collections.unmodifiableSet(itemFlags); }
    @Override public boolean addAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        if (attribute == null || modifier == null) return false;
        attributes.computeIfAbsent(attribute, ignored -> new java.util.ArrayList<>()).add(modifier); return true;
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        java.util.List<org.bukkit.attribute.AttributeModifier> values = attributes.get(attribute);
        return values != null && values.remove(modifier);
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute) {
        return attributes.remove(attribute) != null;
    }

    @Override public com.google.common.collect.Multimap<org.bukkit.attribute.Attribute, org.bukkit.attribute.AttributeModifier> getAttributeModifiers() {
        com.google.common.collect.ArrayListMultimap<org.bukkit.attribute.Attribute, org.bukkit.attribute.AttributeModifier> copy =
            com.google.common.collect.ArrayListMultimap.create();
        attributes.forEach((key, value) -> copy.putAll(key, value));
        return com.google.common.collect.ImmutableMultimap.copyOf(copy);
    }

    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return persistentData;
    }

    @Override
    public SimpleItemMeta clone() {
        try {
            SimpleItemMeta copy = (SimpleItemMeta) super.clone();
            copy.lore = lore == null ? null : new ArrayList<>(lore);
            copy.customModelDataComponent = customModelDataComponent.clone();
            copy.persistentData = persistentData.copy();
            copy.enchantments = new java.util.HashMap<>(enchantments);
            copy.itemFlags = new java.util.HashSet<>(itemFlags);
            copy.attributes = new java.util.HashMap<>();
            attributes.forEach((key, value) -> copy.attributes.put(key, new java.util.ArrayList<>(value)));
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
            && damage == meta.damage
            && java.util.Objects.equals(enchantments, meta.enchantments)
            && java.util.Objects.equals(itemFlags, meta.itemFlags)
            && java.util.Objects.equals(attributes, meta.attributes)
            && java.util.Objects.equals(customModelDataComponent, meta.customModelDataComponent)
            && java.util.Objects.equals(itemModel, meta.itemModel)
            && java.util.Objects.equals(tooltipStyle, meta.tooltipStyle)
            && hideTooltip == meta.hideTooltip;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(displayName, lore, customModelData, unbreakable, damage,
            persistentData, enchantments, itemFlags, attributes, customModelDataComponent,
            itemModel, tooltipStyle, hideTooltip);
    }
}
