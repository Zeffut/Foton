package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;

/** The meta an ItemStack carries when nothing more specific is needed. */
public class SimpleItemMeta implements Damageable {
    private foton.item.LiveItemState liveState = new foton.item.LiveItemState();
    private String nativeFamily;
    private org.bukkit.Material nativeMaterial;

    /** Internal family provenance survives detached clones, independent of public wrappers. */
    public final void setNativeFamily(String family, org.bukkit.Material material) {
        nativeFamily = family;
        nativeMaterial = material;
    }

    public final String nativeFamily() {
        if (nativeFamily != null) return nativeFamily;
        if (this instanceof BlockStateMeta) return "BLOCK_STATE";
        if (this instanceof BookMeta) return "BOOK_WRITABLE";
        if (this instanceof SkullMeta) return "SKULL";
        if (this instanceof LeatherArmorMeta) return "LEATHER_ARMOR";
        if (this instanceof PotionMeta) return "POTION";
        if (this instanceof MapMeta) return "MAP";
        if (this instanceof FireworkMeta) return "FIREWORK";
        if (this instanceof FireworkEffectMeta) return "FIREWORK_EFFECT";
        if (this instanceof EnchantmentStorageMeta) return "ENCHANTMENT_STORAGE";
        if (this instanceof BannerMeta) return "BANNER";
        if (this instanceof CrossbowMeta) return "CROSSBOW";
        if (this instanceof SuspiciousStewMeta) return "SUSPICIOUS_STEW";
        if (this instanceof BundleMeta) return "BUNDLE";
        return "BASE";
    }

    public final org.bukkit.Material nativeMaterial() { return nativeMaterial; }

    /** The legacy codecs have no durable representation for a live component snapshot. */
    public final void requireLegacyPersistence(boolean binary) {
        if (liveState.hasNativeBase()) throw new UnsupportedOperationException("live item snapshot persistence");
        if (getClass() != SimpleItemMeta.class) throw new UnsupportedOperationException("specialized item metadata persistence");
        if (persistentData.hasRetainedItemState() || !attributes.isEmpty() || itemModel != null
                || tooltipStyle != null || hideTooltip || !customModelDataComponent.getFloats().isEmpty()
                || !customModelDataComponent.getFlags().isEmpty() || !customModelDataComponent.getStrings().isEmpty()
                || !customModelDataComponent.getColors().isEmpty())
            throw new UnsupportedOperationException("unrepresented item metadata persistence");
        if (displayNameComponent != null && !displayNameComponent.equals(net.kyori.adventure.text.Component.text(displayName)))
            throw new UnsupportedOperationException("rich item name persistence");
        if (!binary && (!enchantments.isEmpty() || !itemFlags.isEmpty() || damage > Short.MAX_VALUE))
            throw new UnsupportedOperationException("unrepresented item map fields");
        if (binary && (enchantments.size() > 256 || lore != null && (lore.size() > 1024 || lore.contains(null))))
            throw new UnsupportedOperationException("unrepresented legacy binary metadata");
    }

    @Override public java.util.Map<String, Object> serialize() {
        requireLegacyPersistence(false);
        if (displayNameComponent != null || lore != null || customModelData != null || unbreakable || damage != 0)
            throw new UnsupportedOperationException("nonempty standalone metadata persistence");
        return java.util.Map.of();
    }

    /** Internal bridge hydration replaces the journal after all projected setters ran. */
    public final void attachNativeState(foton.item.ItemTransfer transfer) {
        liveState = new foton.item.LiveItemState(transfer);
    }

    public final foton.item.LiveItemState nativeState() { return liveState; }

    protected final void changed(String component, boolean present) {
        liveState.record(component, present ? foton.item.ComponentEdits.Operation.SET
            : foton.item.ComponentEdits.Operation.REMOVE);
    }

    protected final void unsupportedChange(String capability) {
        changed("unsupported:" + capability, true);
    }
    private foton.FotonPersistentDataContainer persistentData = new foton.FotonPersistentDataContainer();
    private String displayName;
    private net.kyori.adventure.text.Component displayNameComponent;
    private List<String> lore;
    private Integer customModelData;
    private boolean unbreakable;
    private int damage;
    private org.bukkit.inventory.meta.components.CustomModelDataComponent customModelDataComponent = new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent();
    private org.bukkit.NamespacedKey itemModel;
    private org.bukkit.NamespacedKey tooltipStyle;
    private boolean hideTooltip;

    public SimpleItemMeta() {
        persistentData.setMutationListener(() -> changed("custom_data", true));
    }

    @Override public int getDamage() { return damage; }
    @Override public void setDamage(int value) {
        if (value < 0) throw new IllegalArgumentException("Damage cannot be negative");
        damage = value;
        changed("damage", true);
    }
    private java.util.Map<org.bukkit.enchantments.Enchantment, Integer> enchantments = new java.util.HashMap<>();
    private java.util.Set<org.bukkit.inventory.ItemFlag> itemFlags = new java.util.HashSet<>();
    private java.util.Map<org.bukkit.attribute.Attribute, java.util.List<org.bukkit.attribute.AttributeModifier>> attributes = new java.util.HashMap<>();

    @Override
    public boolean hasDisplayName() {
        return displayNameComponent != null;
    }

    @Override
    public String getDisplayName() {
        return displayName == null ? "" : displayName;
    }

    @Override
    public void setDisplayName(String name) {
        this.displayName = name;
        this.displayNameComponent = name == null ? null : net.kyori.adventure.text.Component.text(name);
        changed("custom_name", name != null);
    }

    @Override public net.kyori.adventure.text.Component displayName() {
        return displayNameComponent;
    }

    @Override public void displayName(net.kyori.adventure.text.Component value) {
        displayNameComponent = value;
        displayName = value == null ? null : foton.ComponentJson.plain(value);
        changed("custom_name", value != null);
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
        changed("lore", lore != null && !lore.isEmpty());
    }

    @Override
    public boolean hasCustomModelData() {
        return customModelData != null || !customModelDataComponent.getFloats().isEmpty();
    }

    @Override
    public int getCustomModelData() {
        if (customModelData != null) return customModelData;
        if (customModelDataComponent.getFloats().isEmpty()) {
            throw new IllegalStateException("no custom model data; check hasCustomModelData");
        }
        return customModelDataComponent.getFloats().get(0).intValue();
    }

    @Override public org.bukkit.inventory.meta.components.CustomModelDataComponent getCustomModelDataComponent() { return customModelDataComponent.clone(); }
    @Override public void setCustomModelDataComponent(org.bukkit.inventory.meta.components.CustomModelDataComponent component) { customModelData = null; customModelDataComponent = component == null ? new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent() : component.clone(); changed("custom_model_data", component != null); }
    @Override public boolean hasItemModel() { return itemModel != null; }
    @Override public org.bukkit.NamespacedKey getItemModel() { return itemModel; }
    @Override public void setItemModel(org.bukkit.NamespacedKey key) { itemModel = key; changed("item_model", key != null); }
    @Override public boolean hasTooltipStyle() { return tooltipStyle != null; }
    @Override public org.bukkit.NamespacedKey getTooltipStyle() { return tooltipStyle; }
    @Override public void setTooltipStyle(org.bukkit.NamespacedKey key) { tooltipStyle = key; changed("tooltip_style", key != null); }
    @Override public boolean isHideTooltip() { return hideTooltip; }
    @Override public void setHideTooltip(boolean hide) { hideTooltip = hide; changed("tooltip_display", true); }

    @Override
    public void setCustomModelData(Integer data) {
        this.customModelData = data;
        customModelDataComponent = new org.bukkit.inventory.meta.components.SimpleCustomModelDataComponent();
        changed("custom_model_data", data != null);
    }

    @Override
    public boolean isUnbreakable() {
        return unbreakable;
    }

    @Override
    public void setUnbreakable(boolean unbreakable) {
        this.unbreakable = unbreakable;
        changed("unbreakable", unbreakable);
    }

    @Override
    public boolean removeEnchant(org.bukkit.enchantments.Enchantment enchantment) {
        if (enchantments.remove(enchantment) == null) return false;
        changed("enchantments", true);
        return true;
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
        if (previous == null || previous != level) changed("enchantments", true);
        return previous == null || previous != level;
    }

    @Override public void addItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null && itemFlags.add(flag)) unsupportedChange("item_flags");
    }

    @Override public void removeItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null && itemFlags.remove(flag)) unsupportedChange("item_flags");
    }

    @Override public boolean hasItemFlag(org.bukkit.inventory.ItemFlag flag) { return itemFlags.contains(flag); }
    @Override public java.util.Set<org.bukkit.inventory.ItemFlag> getItemFlags() { return java.util.Collections.unmodifiableSet(itemFlags); }
    @Override public boolean addAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        if (attribute == null || modifier == null) return false;
        attributes.computeIfAbsent(attribute, ignored -> new java.util.ArrayList<>()).add(modifier); unsupportedChange("attribute_modifiers"); return true;
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        java.util.List<org.bukkit.attribute.AttributeModifier> values = attributes.get(attribute);
        if (values == null || !values.remove(modifier)) return false;
        unsupportedChange("attribute_modifiers"); return true;
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute) {
        if (attributes.remove(attribute) == null) return false;
        unsupportedChange("attribute_modifiers"); return true;
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
            copy.liveState = liveState.copy();
            copy.lore = lore == null ? null : new ArrayList<>(lore);
            copy.customModelDataComponent = customModelDataComponent.clone();
            copy.persistentData = persistentData.copy();
            copy.persistentData.setMutationListener(() -> copy.changed("custom_data", true));
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
            && java.util.Objects.equals(displayNameComponent, meta.displayNameComponent)
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
        return java.util.Objects.hash(displayName, displayNameComponent, lore, customModelData, unbreakable, damage,
            persistentData, enchantments, itemFlags, attributes, customModelDataComponent,
            itemModel, tooltipStyle, hideTooltip);
    }
}
