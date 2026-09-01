package org.bukkit.block.data;

import java.util.Locale;
import org.bukkit.Material;

/** Block data as the string Foton hands over, parsed only as far as needed. */
public final class SimpleBlockData implements BlockData {
    private final String text;
    private final Material material;

    public SimpleBlockData(String text) {
        this.text = text == null ? "minecraft:air" : text;
        int bracket = this.text.indexOf('[');
        String name = bracket < 0 ? this.text : this.text.substring(0, bracket);
        Material found = Material.matchMaterial(name);
        this.material = found == null ? Material.AIR : found;
    }

    @Override
    public Material getMaterial() {
        return material;
    }

    @Override
    public String getAsString() {
        return text;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof SimpleBlockData data
            && text.toLowerCase(Locale.ROOT).equals(data.text.toLowerCase(Locale.ROOT));
    }

    @Override
    public int hashCode() {
        return text.toLowerCase(Locale.ROOT).hashCode();
    }

    @Override
    public String toString() {
        return text;
    }
}
