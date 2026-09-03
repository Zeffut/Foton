package org.bukkit.configuration;

import java.util.List;
import java.util.Set;

/** A tree of values addressed by dotted paths.
 *
 * Every getter has a defined answer for a path that is not there, and the
 * answer differs by type: a missing String is null, a missing int is 0, and a
 * missing String list is an empty list rather than null. Plugins lean on that
 * last one without checking, so it is not a detail.
 */
public interface ConfigurationSection {
    Set<String> getKeys(boolean deep);

    default java.util.Map<String, Object> getValues(boolean deep) {
        java.util.LinkedHashMap<String, Object> values = new java.util.LinkedHashMap<>();
        for (String key : getKeys(deep)) {
            Object value = get(key);
            if (value != null) values.put(key, value);
        }
        return values;
    }



    boolean contains(String path);

    boolean isSet(String path);

    String getCurrentPath();

    String getName();

    Configuration getRoot();

    ConfigurationSection getParent();

    Object get(String path);

    Object get(String path, Object def);

    void set(String path, Object value);

    ConfigurationSection createSection(String path);

    String getString(String path);

    String getString(String path, String def);

    boolean isString(String path);

    int getInt(String path);

    int getInt(String path, int def);

    boolean isInt(String path);

    boolean getBoolean(String path);

    boolean getBoolean(String path, boolean def);

    boolean isBoolean(String path);

    double getDouble(String path);

    double getDouble(String path, double def);

    boolean isDouble(String path);

    long getLong(String path);

    long getLong(String path, long def);

    boolean isLong(String path);

    List<?> getList(String path);

    List<?> getList(String path, List<?> def);

    boolean isList(String path);

    List<String> getStringList(String path);

    List<Integer> getIntegerList(String path);

    List<Double> getDoubleList(String path);

    List<Boolean> getBooleanList(String path);

    default org.bukkit.inventory.ItemStack getItemStack(String path) {
        Object value = get(path);
        return value instanceof org.bukkit.inventory.ItemStack ? ((org.bukkit.inventory.ItemStack) value).clone() : null;
    }

    default org.bukkit.inventory.ItemStack getItemStack(String path, org.bukkit.inventory.ItemStack def) {
        org.bukkit.inventory.ItemStack value = getItemStack(path);
        return value == null ? def : value;
    }

    ConfigurationSection getConfigurationSection(String path);

    boolean isConfigurationSection(String path);

    void addDefault(String path, Object value);
}
