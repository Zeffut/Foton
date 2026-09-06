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
}
