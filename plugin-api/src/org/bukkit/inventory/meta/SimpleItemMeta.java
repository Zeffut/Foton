package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;

/** The meta an ItemStack carries when nothing more specific is needed.
 *
 * <p>Besides Bukkit's fields it holds the vanilla components Paper exposes on
 * every meta -- glint override, stack size, use cooldown, tooltip display --
 * and those some items carry whatever their meta type (a shield's banner
 * patterns and base color, an armor trim, a horn's instrument, custom data).
 * All of them cross to the server with the item; see
 * {@code foton.FotonInventory#encode}.</p>
 */
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
        if (this instanceof ArmorMeta) return "ARMOR";
        if (this instanceof MusicInstrumentMeta) return "MUSIC_INSTRUMENT";
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
        java.util.Set<String> representedHidden = new java.util.HashSet<>();
        if (binary) for (org.bukkit.inventory.ItemFlag flag : getItemFlags())
            representedHidden.addAll(FLAG_COMPONENTS.get(flag));
        if (!hiddenComponents.equals(representedHidden) || glintOverride != null || maxStackSize != null
                || useCooldown != null || bannerPatterns != null || baseColor != null || trim != null
                || instrument != null || !otherCustomData.isEmpty())
            throw new UnsupportedOperationException("unrepresented extended metadata persistence");
        if (lore != null && lore.stream().anyMatch(line -> !line.equals(net.kyori.adventure.text.Component.text(legacy(line)))))
            throw new UnsupportedOperationException("rich item lore persistence");
        if (persistentData.hasRetainedItemState() || !attributes.isEmpty() || itemModel != null
                || tooltipStyle != null || hideTooltip || !customModelDataComponent.getFloats().isEmpty()
                || !customModelDataComponent.getFlags().isEmpty() || !customModelDataComponent.getStrings().isEmpty()
                || !customModelDataComponent.getColors().isEmpty())
            throw new UnsupportedOperationException("unrepresented item metadata persistence");
        if (displayName != null && !displayName.equals(net.kyori.adventure.text.Component.text(legacy(displayName))))
            throw new UnsupportedOperationException("rich item name persistence");
        if (!binary && (!enchantments.isEmpty() || !getItemFlags().isEmpty() || damage > Short.MAX_VALUE))
            throw new UnsupportedOperationException("unrepresented item map fields");
        if (binary && (enchantments.size() > 256 || lore != null && (lore.size() > 1024 || lore.contains(null))))
            throw new UnsupportedOperationException("unrepresented legacy binary metadata");
    }

    @Override public java.util.Map<String, Object> serialize() {
        requireLegacyPersistence(false);
        if (displayName != null || lore != null || customModelData != null || unbreakable || damage != 0)
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
    /** custom_name, a component so color and formatting survive; null when unnamed. */
    private net.kyori.adventure.text.Component displayName;
    /** lore, one component per line; null when the item has none. */
    private List<net.kyori.adventure.text.Component> lore;
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
    private java.util.Map<org.bukkit.attribute.Attribute, java.util.List<org.bukkit.attribute.AttributeModifier>> attributes = new java.util.HashMap<>();

    @Override
    public boolean hasDisplayName() {
        return displayName != null;
    }

    /** The name as section-sign text, which is what Bukkit's string API speaks. */
    @Override
    public String getDisplayName() {
        return displayName == null ? "" : legacy(displayName);
    }

    /** A string name is kept as text: the client draws its section-sign codes itself. */
    @Override
    public void setDisplayName(String name) {
        this.displayName = name == null || name.isEmpty() ? null : net.kyori.adventure.text.Component.text(name);
        changed("custom_name", displayName != null);
    }

    @Override
    public net.kyori.adventure.text.Component displayName() {
        return displayName;
    }

    @Override
    public void displayName(net.kyori.adventure.text.Component name) {
        this.displayName = name;
        changed("custom_name", name != null);
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
        return lore == null ? null : lore.stream().map(SimpleItemMeta::legacy).collect(java.util.stream.Collectors.toCollection(ArrayList::new));
    }

    @Override
    public void setLore(List<String> lines) {
        this.lore = lines == null ? null : lines.stream()
            .map(line -> (net.kyori.adventure.text.Component) net.kyori.adventure.text.Component.text(line == null ? "" : line))
            .collect(java.util.stream.Collectors.toCollection(ArrayList::new));
        changed("lore", lore != null && !lore.isEmpty());
    }

    @Override
    public List<net.kyori.adventure.text.Component> lore() {
        return lore == null ? null : new ArrayList<>(lore);
    }

    @Override
    public void lore(List<net.kyori.adventure.text.Component> lines) {
        this.lore = lines == null ? null : lines.stream()
            .map(line -> line == null ? net.kyori.adventure.text.Component.empty() : line)
            .collect(java.util.stream.Collectors.toCollection(ArrayList::new));
        changed("lore", lore != null && !lore.isEmpty());
    }

    /** Plain text stays itself; styled text becomes section-sign codes. */
    private static String legacy(net.kyori.adventure.text.Component component) {
        if (component instanceof net.kyori.adventure.text.TextComponent text
                && text.children().isEmpty() && text.style().isEmpty()) {
            return text.content();
        }
        return foton.ComponentJson.legacy(component);
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
    @Override public void setHideTooltip(boolean hide) { hideTooltip = hide; changed("tooltip_visibility", true); }

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
    public boolean hasEnchant(org.bukkit.enchantments.Enchantment enchantment) {
        return enchantment != null && enchantments.containsKey(enchantment);
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
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) if (flag != null) hiddenComponents.addAll(FLAG_COMPONENTS.get(flag));
        changed("tooltip_display", true);
    }

    /** As Paper: a flag's components are shown again only when all of them were hidden. */
    @Override public void removeItemFlags(org.bukkit.inventory.ItemFlag... flags) {
        if (flags != null) for (org.bukkit.inventory.ItemFlag flag : flags) {
            if (flag != null && hasItemFlag(flag)) FLAG_COMPONENTS.get(flag).forEach(hiddenComponents::remove);
        }
        changed("tooltip_display", true);
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
        changed("tooltip_display", true);
    }

    @Override public boolean hasEnchantmentGlintOverride() { return glintOverride != null; }
    @Override public Boolean getEnchantmentGlintOverride() {
        if (glintOverride == null) throw new IllegalStateException("no enchantment glint override; check hasEnchantmentGlintOverride");
        return glintOverride;
    }
    @Override public void setEnchantmentGlintOverride(Boolean override) { glintOverride = override; changed("enchantment_glint_override", override != null); }

    @Override public boolean hasMaxStackSize() { return maxStackSize != null; }
    @Override public int getMaxStackSize() {
        if (maxStackSize == null) throw new IllegalStateException("no max stack size; check hasMaxStackSize");
        return maxStackSize;
    }
    /** Vanilla's range, 1 to 99, as Paper checks it. */
    @Override public void setMaxStackSize(Integer max) {
        if (max != null && (max < 1 || max > 99)) throw new IllegalArgumentException("max_stack_size must be in 1..99, got " + max);
        maxStackSize = max;
        changed("max_stack_size", max != null);
    }

    @Override public boolean hasUseCooldown() { return useCooldown != null; }
    /** A copy, as Paper's: changes reach the item through setUseCooldown. */
    @Override public org.bukkit.inventory.meta.components.UseCooldownComponent getUseCooldown() {
        return useCooldown == null ? new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(0, null) : useCooldown.copy();
    }
    @Override public void setUseCooldown(org.bukkit.inventory.meta.components.UseCooldownComponent cooldown) {
        if (cooldown == null) { useCooldown = null; changed("use_cooldown", false); return; }
        if (!Float.isFinite(cooldown.getCooldownSeconds()) || !(cooldown.getCooldownSeconds() > 0)) throw new IllegalArgumentException("a use cooldown must last a positive finite time");
        useCooldown = new org.bukkit.inventory.meta.components.SimpleUseCooldownComponent(cooldown.getCooldownSeconds(), cooldown.getCooldownGroup());
        changed("use_cooldown", true);
    }

    /** banner_patterns, bottom layer first; null when absent. */
    public java.util.List<org.bukkit.block.banner.Pattern> getBannerPatternsComponent() {
        return bannerPatterns == null ? null : java.util.List.copyOf(bannerPatterns);
    }
    public void setBannerPatternsComponent(java.util.List<org.bukkit.block.banner.Pattern> patterns) {
        bannerPatterns = patterns == null ? null : new java.util.ArrayList<>(patterns);
        changed("banner_patterns", patterns != null);
    }
    /** base_color, as a painted shield carries it; null when absent. */
    public org.bukkit.DyeColor getBaseColorComponent() { return baseColor; }
    public void setBaseColorComponent(org.bukkit.DyeColor color) { baseColor = color; changed("base_color", color != null); }
    public org.bukkit.inventory.meta.trim.ArmorTrim getTrimComponent() { return trim; }
    public void setTrimComponent(org.bukkit.inventory.meta.trim.ArmorTrim value) { trim = value; changed("trim", value != null); }
    public org.bukkit.MusicInstrument getInstrumentComponent() { return instrument; }
    public void setInstrumentComponent(org.bukkit.MusicInstrument value) { instrument = value; changed("instrument", value != null); }

    /** The item's whole custom_data: other data as it came, and this meta's
     * persistent data under PublicBukkitValues, where Paper keeps it. */
    public java.util.Map<String, Object> getCustomData() {
        java.util.Map<String, Object> data = copyCustomData(otherCustomData);
        java.util.Map<String, Object> bukkit = persistentData.toNbt();
        if (!bukkit.isEmpty()) data.put("PublicBukkitValues", bukkit);
        return data;
    }
    @SuppressWarnings("unchecked")
    public void setCustomData(java.util.Map<String, Object> data) {
        otherCustomData = copyCustomData(data == null ? java.util.Map.of() : data);
        Object bukkit = otherCustomData.get("PublicBukkitValues");
        if (bukkit instanceof java.util.Map<?, ?>) otherCustomData.remove("PublicBukkitValues");
        persistentData = bukkit instanceof java.util.Map<?, ?> values
            ? foton.FotonPersistentDataContainer.fromNbt((java.util.Map<String, Object>) values)
            : new foton.FotonPersistentDataContainer();
        persistentData.setMutationListener(() -> changed("custom_data", true));
        changed("custom_data", !otherCustomData.isEmpty() || !persistentData.toNbt().isEmpty());
    }

    private static java.util.Map<String, Object> copyCustomData(java.util.Map<String, Object> data) {
        java.util.Map<String, Object> copy = new java.util.TreeMap<>();
        data.forEach((key, value) -> copy.put(key, copyCustomValue(value)));
        return copy;
    }

    private static Object copyCustomValue(Object value) {
        if (value instanceof byte[] bytes) return bytes.clone();
        if (value instanceof int[] integers) return integers.clone();
        if (value instanceof long[] longs) return longs.clone();
        if (value instanceof java.util.Map<?, ?> map) {
            java.util.Map<String, Object> copy = new java.util.TreeMap<>();
            map.forEach((key, entry) -> copy.put((String) key, copyCustomValue(entry)));
            return copy;
        }
        if (value instanceof java.util.List<?> list)
            return list.stream().map(SimpleItemMeta::copyCustomValue).collect(java.util.stream.Collectors.toCollection(ArrayList::new));
        return value; // NBT scalar wrappers and strings are immutable.
    }
    @Override public boolean addAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        if (attribute == null || modifier == null) return false;
        attributes.computeIfAbsent(attribute, ignored -> new java.util.ArrayList<>()).add(modifier); unsupportedChange("attribute_modifiers"); return true;
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute, org.bukkit.attribute.AttributeModifier modifier) {
        if (nativeState().hasNativeBase()) unsupportedChange("attribute_modifiers");
        java.util.List<org.bukkit.attribute.AttributeModifier> values = attributes.get(attribute);
        if (values == null || !values.remove(modifier)) return false;
        unsupportedChange("attribute_modifiers"); return true;
    }
    @Override public boolean removeAttributeModifier(org.bukkit.attribute.Attribute attribute) {
        if (nativeState().hasNativeBase()) unsupportedChange("attribute_modifiers");
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
            copy.hiddenComponents = new java.util.LinkedHashSet<>(hiddenComponents);
            copy.useCooldown = useCooldown == null ? null : useCooldown.copy();
            copy.bannerPatterns = bannerPatterns == null ? null : new java.util.ArrayList<>(bannerPatterns);
            copy.otherCustomData = copyCustomData(otherCustomData);
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
