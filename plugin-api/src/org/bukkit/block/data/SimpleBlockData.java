package org.bukkit.block.data;

import java.util.Locale;
import org.bukkit.Material;

/** Block data as the string Foton hands over, parsed only as far as needed. */
public class SimpleBlockData implements BlockData {
    protected String text;
    private final Material material;

    public SimpleBlockData(String text) {
        this.text = text == null ? "minecraft:air" : text;
        int bracket = this.text.indexOf('[');
        String name = bracket < 0 ? this.text : this.text.substring(0, bracket);
        Material found = Material.matchMaterial(name);
        this.material = found == null ? Material.AIR : found;
    }

    @Override
    public BlockData clone() { return PropertyBlockData.of(getAsString()); }

    @Override
    public Material getMaterial() {
        return material;
    }

    @Override
    public String getAsString() {
        return text;
    }

    /** The properties the text lists, in the order it lists them. */
    protected java.util.Map<String, String> properties() {
        java.util.Map<String, String> result = new java.util.LinkedHashMap<>();
        int start = text.indexOf('[');
        if (start < 0) return result;
        for (String entry : text.substring(start + 1, text.length() - 1).split(",")) {
            String[] pair = entry.split("=", 2);
            if (pair.length == 2) result.put(pair[0].trim(), pair[1].trim());
        }
        return result;
    }

    protected String propertyValue(String key) {
        int start = text.indexOf('['); if (start < 0) return "";
        String properties = text.substring(start + 1, text.length() - 1);
        for (String entry : properties.split(",")) { String[] pair = entry.split("=", 2); if (pair.length == 2 && pair[0].trim().equals(key)) return pair[1].trim(); }
        return "";
    }

    protected boolean property(String key) {
        int start = text.indexOf('[');
        if (start < 0) return false;
        String properties = text.substring(start + 1, text.length() - 1);
        for (String entry : properties.split(",")) {
            String[] pair = entry.split("=", 2);
            if (pair.length == 2 && pair[0].trim().equals(key)) {
                return Boolean.parseBoolean(pair[1].trim());
            }
        }
        return false;
    }

    protected void property(String key, String value) {
        String replacement = key + "=" + value;
        int start = text.indexOf('[');
        if (start < 0) {
            text = text + "[" + replacement + "]";
            return;
        }
        int end = text.lastIndexOf(']');
        String[] entries = text.substring(start + 1, end).split(",");
        for (int i = 0; i < entries.length; i++) {
            String[] pair = entries[i].split("=", 2);
            if (pair.length == 2 && pair[0].trim().equals(key)) {
                entries[i] = replacement;
                text = text.substring(0, start + 1) + String.join(",", entries) + text.substring(end);
                return;
            }
        }
        text = text.substring(0, end) + "," + replacement + text.substring(end);
    }

    protected void property(String key, boolean value) {
        String replacement = key + "=" + value;
        int start = text.indexOf('[');
        if (start < 0) {
            text = text + "[" + replacement + "]";
            return;
        }
        int end = text.lastIndexOf(']');
        String[] entries = text.substring(start + 1, end).split(",");
        for (int i = 0; i < entries.length; i++) {
            String[] pair = entries[i].split("=", 2);
            if (pair.length == 2 && pair[0].trim().equals(key)) {
                entries[i] = replacement;
                text = text.substring(0, start + 1) + String.join(",", entries) + text.substring(end);
                return;
            }
        }
        text = text.substring(0, end) + "," + replacement + text.substring(end);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof BlockData data
            && text.equalsIgnoreCase(data.getAsString());
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
