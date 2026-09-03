package io.papermc.paper.datacomponent;

import io.papermc.paper.datacomponent.item.CustomModelData;

/** Built-in component keys supported by Foton. */
public final class DataComponentTypes {
    private DataComponentTypes() { }
    public static final DataComponentType.Valued<CustomModelData> CUSTOM_MODEL_DATA = new DataComponentType.Valued<CustomModelData>() { };
}
