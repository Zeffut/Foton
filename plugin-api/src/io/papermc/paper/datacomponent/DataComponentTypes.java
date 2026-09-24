package io.papermc.paper.datacomponent;

import io.papermc.paper.datacomponent.item.CustomModelData;

/** Built-in component keys supported by Foton. */
public final class DataComponentTypes {
    private DataComponentTypes() { }
    public static final DataComponentType.Valued<CustomModelData> CUSTOM_MODEL_DATA = new DataComponentType.Valued<CustomModelData>() { };

    /** How damaged the item is, counting up from zero.
     *
     * <p>The same number `ItemStack.getDurability` has always carried, reached
     * through Paper's component API. Backed by that field rather than stored
     * separately, so a plugin that sets damage one way and reads it the other
     * sees one item and not two. */
    public static final DataComponentType.Valued<Integer> DAMAGE = new DataComponentType.Valued<Integer>() { };

    // The next three live in the item's meta, like CUSTOM_MODEL_DATA, so a
    // plugin reading them through getItemMeta sees the same item.

    /** A banner's or shield's pattern layers. */
    public static final DataComponentType.Valued<io.papermc.paper.datacomponent.item.BannerPatternLayers> BANNER_PATTERNS =
        new DataComponentType.Valued<io.papermc.paper.datacomponent.item.BannerPatternLayers>() { };
    /** The base colour of a painted shield. */
    public static final DataComponentType.Valued<org.bukkit.DyeColor> BASE_COLOR =
        new DataComponentType.Valued<org.bukkit.DyeColor>() { };
    /** An armor piece's trim. */
    public static final DataComponentType.Valued<io.papermc.paper.datacomponent.item.ItemArmorTrim> TRIM =
        new DataComponentType.Valued<io.papermc.paper.datacomponent.item.ItemArmorTrim>() { };
}
