package org.bukkit.inventory.meta;

import java.util.List;

/** The name, lore and flags carried alongside an item.
 *
 * Bukkit's ItemMeta is a large interface with a dozen subtypes for particular
 * items. This is the part plugins actually reach for -- the display name and
 * the lore -- and it is honest about being that: a plugin asking for a
 * BookMeta gets nothing rather than a stub that quietly loses its pages.
 */
public interface ItemMeta extends Cloneable, org.bukkit.persistence.PersistentDataHolder {
    default Spigot spigot() { return new Spigot(this); }
    class Spigot {
        private final ItemMeta meta;
        protected Spigot() { this(null); }
        protected Spigot(ItemMeta meta) { this.meta = meta; }
        public void setUnbreakable(boolean value) { if (meta != null) meta.setUnbreakable(value); }
        public boolean isUnbreakable() { return meta != null && meta.isUnbreakable(); }
    }
    default org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return new foton.FotonPersistentDataContainer();
    }
    boolean hasDisplayName();

    String getDisplayName();

    void setDisplayName(String name);

    boolean hasLore();

    List<String> getLore();

    void setLore(List<String> lore);
    default net.kyori.adventure.text.Component displayName() {
        return hasDisplayName() ? net.kyori.adventure.text.Component.text(getDisplayName()) : null;
    }
    default void displayName(net.kyori.adventure.text.Component value) {
        setDisplayName(value == null ? null : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(value));
    }
    default java.util.List<net.kyori.adventure.text.Component> lore() {
        List<String> values = getLore();
        if (values == null) return null;
        return values.stream().map(value -> (net.kyori.adventure.text.Component) net.kyori.adventure.text.Component.text(value)).toList();
    }
    default void lore(java.util.List<net.kyori.adventure.text.Component> values) {
        if (values == null) { setLore(null); return; }
        setLore(values.stream().map(value -> value == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(value)).toList());
    }

    boolean hasCustomModelData();

    int getCustomModelData();

    void setCustomModelData(Integer data);

    boolean isUnbreakable();

    void setUnbreakable(boolean unbreakable);

    boolean addEnchant(org.bukkit.enchantments.Enchantment enchantment, int level, boolean ignoreLevelRestriction);
    default boolean removeEnchant(org.bukkit.enchantments.Enchantment enchantment) { return false; }
    default int getEnchantLevel(org.bukkit.enchantments.Enchantment enchantment) { return 0; }
    default java.util.Map<org.bukkit.enchantments.Enchantment, Integer> getEnchants() {
        return java.util.Collections.emptyMap();
    }
    default boolean hasEnchants() { return !getEnchants().isEmpty(); }
    default boolean hasItemFlag(org.bukkit.inventory.ItemFlag flag) { return false; }
    default void addItemFlags(org.bukkit.inventory.ItemFlag... flags) { }
    default void removeItemFlags(org.bukkit.inventory.ItemFlag... flags) { }
    default java.util.Set<org.bukkit.inventory.ItemFlag> getItemFlags() { return java.util.Collections.emptySet(); }
    default boolean addAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) { return false; }
    default boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) { return false; }
    default boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute) { return false; }

    default com.google.common.collect.Multimap<org.bukkit.attribute.Attribute, org.bukkit.attribute.AttributeModifier> getAttributeModifiers() {
        return com.google.common.collect.ImmutableMultimap.of();
    }

    /** Returns whether this meta contains any attribute modifiers. */
    default boolean hasAttributeModifiers() {
        return !getAttributeModifiers().isEmpty();
    }



    default org.bukkit.inventory.meta.components.CustomModelDataComponent getCustomModelDataComponent() { return new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent(); }
    default void setCustomModelDataComponent(org.bukkit.inventory.meta.components.CustomModelDataComponent component) { }
    default boolean hasItemModel() { return false; }
    default org.bukkit.NamespacedKey getItemModel() { return null; }
    default void setItemModel(org.bukkit.NamespacedKey key) { }
    default boolean hasTooltipStyle() { return false; }
    default org.bukkit.NamespacedKey getTooltipStyle() { return null; }
    default void setTooltipStyle(org.bukkit.NamespacedKey key) { }
    default boolean isHideTooltip() { return false; }
    default void setHideTooltip(boolean hide) { }

    ItemMeta clone();
    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
