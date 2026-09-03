package org.bukkit.inventory.meta.components;
import java.util.List;
/** Mutable component backing the modern custom_model_data item component. */
public interface CustomModelDataComponent extends Cloneable {
    List<Float> getFloats(); void setFloats(List<Float> values);
    List<Boolean> getFlags(); void setFlags(List<Boolean> values);
    List<String> getStrings(); void setStrings(List<String> values);
    List<org.bukkit.Color> getColors(); void setColors(List<org.bukkit.Color> values);
    CustomModelDataComponent clone();
}
