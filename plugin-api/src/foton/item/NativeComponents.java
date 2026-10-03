package foton.item;

import io.papermc.paper.datacomponent.item.CustomModelData;

/** Typed JNI construction without routing component values through public persistence. */
public final class NativeComponents {
    private NativeComponents() {}
    public static CustomModelData model(float[] floats, boolean[] flags, String[] strings, int[] colors) {
        CustomModelData.Builder builder = CustomModelData.customModelData();
        for (float value : floats) builder.addFloat(value);
        for (boolean value : flags) builder.addFlag(value);
        for (String value : strings) builder.addString(value);
        for (int value : colors) builder.addColor(value);
        return (CustomModelData) builder.build();
    }
}
