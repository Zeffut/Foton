package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;

/** The meta an ItemStack carries when nothing more specific is needed.
 *
 * <p>Besides Bukkit's fields it holds the vanilla components Paper exposes on
 * every meta -- glint override, stack size, use cooldown, tooltip display --
 * and those some items carry whatever their meta type (a shield's banner
 * patterns and base colour, an armor trim, a horn's instrument, custom data).
 * All of them cross to the server with the item; see
 * {@code foton.FotonInventory#encode}.</p>
 */
public class SimpleItemMeta implements Damageable {
    /** Paper's CraftMetaItem.ITEM_FLAG_EQUIVALENTS: the components each flag hides. */
    private static final java.util.Map<org.bukkit.inventory.ItemFlag, java.util.List<String>> FLAG_COMPONENTS = flagComponents();

    private static java.util.Map<org.bukkit.inventory.ItemFlag, java.util.List<String>> flagComponents() {
        java.util.EnumMap<org.bukkit.inventory.ItemFlag, java.util.List<String>> map = new java.util.EnumMap<>(org.bukkit.inventory.ItemFlag.class);
        map.put(org.bukkit.inventory.ItemFlag.HIDE_ATTRIBUTES, java.util.List.of("minecraft:attribute_modifiers"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_ENCHANTS, java.util.List.of("minecraft:enchantments"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_STORED_ENCHANTS, java.util.List.of("minecraft:stored_enchantments"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_UNBREAKABLE, java.util.List.of("minecraft:unbreakable"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_DYE, java.util.List.of("minecraft:dyed_color"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_ARMOR_TRIM, java.util.List.of("minecraft:trim"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_PLACED_ON, java.util.List.of("minecraft:can_place_on"));
        map.put(org.bukkit.inventory.ItemFlag.HIDE_DESTROYS, java.util.List.of("minecraft:can_break"));
        // Paper's HIDDEN_COMPONENTS_PREVIOUSLY: what the pre-1.21.5
        // hide_additional_tooltip component covered.
        map.put(org.bukkit.inventory.ItemFlag.HIDE_ADDITIONAL_TOOLTIP, java.util.List.of(
            "minecraft:banner_patterns", "minecraft:bees", "minecraft:block_entity_data",
            "minecraft:block_state", "minecraft:bundle_contents", "minecraft:charged_projectiles",
            "minecraft:container", "minecraft:container_loot", "minecraft:firework_explosion",
            "minecraft:fireworks", "minecraft:instrument", "minecraft:jukebox_playable",
            "minecraft:map_id", "minecraft:painting/variant", "minecraft:pot_decorations",
            "minecraft:potion_contents", "minecraft:tropical_fish/pattern",
            "minecraft:written_book_content"));
        return java.util.Collections.unmodifiableMap(map);
    }

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
    /** tooltip_display's hidden components, in order; item flags are read from these. */
    private java.util.LinkedHashSet<String> hiddenComponents = new java.util.LinkedHashSet<>();
    private Boolean glintOverride;
    private Integer maxStackSize;
    private org.bukkit.inventory.meta.components.SimpleUseCooldownComponent useCooldown;
    /** banner_patterns; null when the item has no such component. */
    private java.util.List<org.bukkit.block.banner.Pattern> bannerPatterns;
    private org.bukkit.DyeColor baseColor;
    private org.bukkit.inventory.meta.trim.ArmorTrim trim;
    private org.bukkit.MusicInstrument instrument;
    /** custom_data other than PublicBukkitValues, kept as it came so nothing is lost on the way back. */
    private java.util.Map<String, Object> otherCustomData = new java.util.TreeMap<>();

    @Override public int getDamage() { return damage; }
    @Override public void setDamage(int value) { damage = Math.max(0, value); }
    private java.util.Map<org.bukkit.enchantments.Enchantment, Integer> enchantments = new java.util.HashMap<>();
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
    public boolean hasEnchant(org.bukkit.enchantments.Enchantment enchantment) {
        return enchantment != null && enchantments.containsKey(enchantment);
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
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null) hiddenComponents.addAll(FLAG_COMPONENTS.get(flag));
    }

    /** As Paper: a flag's components are shown again only when all of them were hidden. */
    @Override public void removeItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) {
            if (flag != null && hasItemFlag(flag)) FLAG_COMPONENTS.get(flag).forEach(hiddenComponents::remove);
        }
    }

    @Override public boolean hasItemFlag(org.bukkit.inventory.ItemFlag flag) {
        return flag != null && hiddenComponents.containsAll(FLAG_COMPONENTS.get(flag));
    }

    @Override public java.util.Set<org.bukkit.inventory.ItemFlag> getItemFlags() {
        java.util.EnumSet<org.bukkit.inventory.ItemFlag> flags = java.util.EnumSet.noneOf(org.bukkit.inventory.ItemFlag.class);
        for (org.bukkit.inventory.ItemFlag flag : org.bukkit.inventory.ItemFlag.values()) if (hasItemFlag(flag)) flags.add(flag);
        return java.util.Collections.unmodifiableSet(flags);
    }

    /** The data components the client is told not to show, as registry keys. */
    public java.util.List<String> getHiddenComponents() { return new java.util.ArrayList<>(hiddenComponents); }
    public void setHiddenComponents(java.util.Collection<String> keys) {
        hiddenComponents = keys == null ? new java.util.LinkedHashSet<>() : new java.util.LinkedHashSet<>(keys);
    }

    @Override public boolean hasEnchantmentGlintOverride() { return glintOverride != null; }
    @Override public Boolean getEnchantmentGlintOverride() {
        if (glintOverride == null) throw new IllegalStateException("no enchantment glint override; check hasEnchantmentGlintOverride");
        return glintOverride;
    }
    @Override public void setEnchantmentGlintOverride(Boolean override) { glintOverride = override; }

    @Override public boolean hasMaxStackSize() { return maxStackSize != null; }
    @Override public int getMaxStackSize() {
        if (maxStackSize == null) throw new IllegalStateException("no max stack size; check hasMaxStackSize");
        return maxStackSize;
    }
    /** Vanilla's range, 1 to 99, as Paper checks it. */
    @Override public void setMaxStackSize(Integer max) {
        if (max != null && (max < 1 || max > 99)) throw new IllegalArgumentException("max_stack_size must be in 1..99, got " + max);
        maxStackSize = max;
    }

    @Override public boolean hasUseCooldown() { return useCooldown != null; }
    /** A copy, as Paper's: changes reach the item through setUseCooldown. */
    @Override public org.bukkit.inventory.meta.components.UseCooldownComponent getUseCooldown() {
        return useCooldown == null ? new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(0, null) : useCooldown.copy();
    }
    @Override public void setUseCooldown(org.bukkit.inventory.meta.components.UseCooldownComponent cooldown) {
        if (cooldown == null) { useCooldown = null; return; }
        if (!(cooldown.getCooldownSeconds() > 0)) throw new IllegalArgumentException("a use cooldown must last a positive time");
        useCooldown = new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(cooldown.getCooldownSeconds(), cooldown.getCooldownGroup());
    }

    /** banner_patterns, bottom layer first; null when absent. */
    public java.util.List<org.bukkit.block.banner.Pattern> getBannerPatternsComponent() {
        return bannerPatterns == null ? null : java.util.List.copyOf(bannerPatterns);
    }
    public void setBannerPatternsComponent(java.util.List<org.bukkit.block.banner.Pattern> patterns) {
        bannerPatterns = patterns == null ? null : new java.util.ArrayList<>(patterns);
    }
    /** base_color, as a painted shield carries it; null when absent. */
    public org.bukkit.DyeColor getBaseColorComponent() { return baseColor; }
    public void setBaseColorComponent(org.bukkit.DyeColor color) { baseColor = color; }
    public org.bukkit.inventory.meta.trim.ArmorTrim getTrimComponent() { return trim; }
    public void setTrimComponent(org.bukkit.inventory.meta.trim.ArmorTrim value) { trim = value; }
    public org.bukkit.MusicInstrument getInstrumentComponent() { return instrument; }
    public void setInstrumentComponent(org.bukkit.MusicInstrument value) { instrument = value; }

    /** The item's whole custom_data: other data as it came, and this meta's
     * persistent data under PublicBukkitValues, where Paper keeps it. */
    public java.util.Map<String, Object> getCustomData() {
        java.util.Map<String, Object> data = new java.util.TreeMap<>(otherCustomData);
        if (!persistentData.isEmpty()) data.put("PublicBukkitValues", persistentData.toNbt());
        return data;
    }
    @SuppressWarnings("unchecked")
    public void setCustomData(java.util.Map<String, Object> data) {
        otherCustomData = new java.util.TreeMap<>(data == null ? java.util.Map.of() : data);
        Object bukkit = otherCustomData.remove("PublicBukkitValues");
        persistentData = bukkit instanceof java.util.Map<?, ?> values
            ? foton.FotonPersistentDataContainer.fromNbt((java.util.Map<String, Object>) values)
            : new foton.FotonPersistentDataContainer();
    }
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
            copy.hiddenComponents = new java.util.LinkedHashSet<>(hiddenComponents);
            copy.useCooldown = useCooldown == null ? null : useCooldown.copy();
            copy.bannerPatterns = bannerPatterns == null ? null : new java.util.ArrayList<>(bannerPatterns);
            copy.otherCustomData = new java.util.TreeMap<>(otherCustomData);
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
            && java.util.Objects.equals(hiddenComponents, meta.hiddenComponents)
            && java.util.Objects.equals(glintOverride, meta.glintOverride)
            && java.util.Objects.equals(maxStackSize, meta.maxStackSize)
            && java.util.Objects.equals(useCooldown, meta.useCooldown)
            && java.util.Objects.equals(bannerPatterns, meta.bannerPatterns)
            && baseColor == meta.baseColor
            && java.util.Objects.equals(trim, meta.trim)
            && instrument == meta.instrument
            && foton.Snbt.write(otherCustomData).equals(foton.Snbt.write(meta.otherCustomData))
            && java.util.Objects.equals(attributes, meta.attributes)
            && java.util.Objects.equals(customModelDataComponent, meta.customModelDataComponent)
            && java.util.Objects.equals(itemModel, meta.itemModel)
            && java.util.Objects.equals(tooltipStyle, meta.tooltipStyle)
            && hideTooltip == meta.hideTooltip;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(displayName, lore, customModelData, unbreakable, damage,
            persistentData, enchantments, hiddenComponents, attributes, customModelDataComponent,
            itemModel, tooltipStyle, hideTooltip, glintOverride, maxStackSize, useCooldown,
            bannerPatterns, baseColor, trim, instrument, foton.Snbt.write(otherCustomData));
    }
}
