package org.bukkit.configuration;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** A section backed by a map, which is every section Foton has.
 *
 * Paths are split on the root's separator and walked a level at a time, so
 * `a.b.c` is three lookups, and a missing `a` answers for the whole path
 * rather than throwing.
 */
public class MemorySection implements ConfigurationSection {
    protected final Map<String, Object> map = new LinkedHashMap<>();
    private final Configuration root;
    private final ConfigurationSection parent;
    private final String path;
    private final String fullPath;

    /** For a root configuration, which is its own root and has no parent. */
    protected MemorySection() {
        if (!(this instanceof Configuration)) {
            throw new IllegalStateException("only a Configuration may have no parent");
        }
        this.root = (Configuration) this;
        this.parent = null;
        this.path = "";
        this.fullPath = "";
    }

    protected MemorySection(ConfigurationSection parent, String path) {
        this.parent = parent;
        this.path = path;
        this.root = parent.getRoot();
        String parentPath = parent.getCurrentPath();
        this.fullPath = parentPath.isEmpty() ? path : parentPath + separator() + path;
    }

    private char separator() {
        ConfigurationOptions options = root == null ? null : root.options();
        return options == null ? '.' : options.pathSeparator();
    }

    @Override
    public Set<String> getKeys(boolean deep) {
        Set<String> keys = new LinkedHashSet<>();
        collectKeys(this, "", deep, keys);
        return keys;
    }

    private void collectKeys(
            ConfigurationSection section, String prefix, boolean deep, Set<String> into) {
        if (!(section instanceof MemorySection)) {
            return;
        }
        for (Map.Entry<String, Object> entry : ((MemorySection) section).map.entrySet()) {
            String key = prefix + entry.getKey();
            into.add(key);
            if (deep && entry.getValue() instanceof ConfigurationSection) {
                collectKeys(
                    (ConfigurationSection) entry.getValue(), key + separator(), true, into);
            }
        }
    }

    @Override
    public boolean contains(String path) {
        return get(path) != null;
    }

    @Override
    public boolean isSet(String path) {
        return get(path) != null;
    }

    @Override
    public String getCurrentPath() {
        return fullPath;
    }

    @Override
    public String getName() {
        return path;
    }

    @Override
    public Configuration getRoot() {
        return root;
    }

    @Override
    public ConfigurationSection getParent() {
        return parent;
    }

    @Override
    public Object get(String path) {
        return get(path, null);
    }

    @Override
    public Object get(String path, Object def) {
        if (path == null || path.isEmpty()) {
            return this;
        }
        char separator = separator();
        MemorySection section = this;
        int start = 0;
        int next;
        while ((next = path.indexOf(separator, start)) != -1) {
            Object child = section.map.get(path.substring(start, next));
            if (!(child instanceof MemorySection)) {
                return fallback(path, def);
            }
            section = (MemorySection) child;
            start = next + 1;
        }
        Object value = section.map.get(path.substring(start));
        return value == null ? fallback(path, def) : value;
    }

    /** What a missing path answers: the caller's default, else the root's. */
    private Object fallback(String path, Object def) {
        if (def != null) {
            return def;
        }
        Configuration defaults = root == null ? null : root.getDefaults();
        if (defaults == null || defaults == root) {
            return null;
        }
        return defaults.get(fullPath.isEmpty() ? path : fullPath + separator() + path);
    }

    @Override
    public void set(String path, Object value) {
        char separator = separator();
        MemorySection section = this;
        int start = 0;
        int next;
        while ((next = path.indexOf(separator, start)) != -1) {
            String key = path.substring(start, next);
            Object child = section.map.get(key);
            if (!(child instanceof MemorySection)) {
                if (value == null) {
                    // Nothing to remove, and no reason to build the way there.
                    return;
                }
                child = new MemorySection(section, key);
                section.map.put(key, child);
            }
            section = (MemorySection) child;
            start = next + 1;
        }
        String key = path.substring(start);
        if (value == null) {
            section.map.remove(key);
        } else {
            section.map.put(key, value);
        }
    }

    @Override
    public ConfigurationSection createSection(String path) {
        char separator = separator();
        MemorySection section = this;
        int start = 0;
        int next;
        while ((next = path.indexOf(separator, start)) != -1) {
            String key = path.substring(start, next);
            Object child = section.map.get(key);
            if (!(child instanceof MemorySection)) {
                child = new MemorySection(section, key);
                section.map.put(key, child);
            }
            section = (MemorySection) child;
            start = next + 1;
        }
        String key = path.substring(start);
        MemorySection created = new MemorySection(section, key);
        section.map.put(key, created);
        return created;
    }

    @Override
    public String getString(String path) {
        return getString(path, null);
    }

    @Override
    public String getString(String path, String def) {
        Object value = get(path);
        return value == null ? def : value.toString();
    }

    @Override
    public boolean isString(String path) {
        return get(path) instanceof String;
    }

    @Override
    public int getInt(String path) {
        return getInt(path, 0);
    }

    @Override
    public int getInt(String path, int def) {
        Object value = get(path);
        return value instanceof Number ? ((Number) value).intValue() : def;
    }

    @Override
    public boolean isInt(String path) {
        return get(path) instanceof Integer;
    }

    @Override
    public boolean getBoolean(String path) {
        return getBoolean(path, false);
    }

    @Override
    public boolean getBoolean(String path, boolean def) {
        Object value = get(path);
        return value instanceof Boolean ? (Boolean) value : def;
    }

    @Override
    public boolean isBoolean(String path) {
        return get(path) instanceof Boolean;
    }

    @Override
    public double getDouble(String path) {
        return getDouble(path, 0);
    }

    @Override
    public double getDouble(String path, double def) {
        Object value = get(path);
        return value instanceof Number ? ((Number) value).doubleValue() : def;
    }

    @Override
    public boolean isDouble(String path) {
        return get(path) instanceof Double;
    }

    @Override
    public long getLong(String path) {
        return getLong(path, 0);
    }

    @Override
    public long getLong(String path, long def) {
        Object value = get(path);
        return value instanceof Number ? ((Number) value).longValue() : def;
    }

    @Override
    public boolean isLong(String path) {
        return get(path) instanceof Long;
    }

    @Override
    public List<?> getList(String path) {
        return getList(path, null);
    }

    @Override
    public List<?> getList(String path, List<?> def) {
        Object value = get(path);
        return value instanceof List ? (List<?>) value : def;
    }

    @Override
    public boolean isList(String path) {
        return get(path) instanceof List;
    }

    /** An absent list is an empty one, not null. Plugins do not check. */
    @Override
    public List<String> getStringList(String path) {
        List<String> out = new ArrayList<>();
        List<?> list = getList(path);
        if (list == null) {
            return out;
        }
        for (Object entry : list) {
            if (entry != null) {
                out.add(entry.toString());
            }
        }
        return out;
    }

    @Override
    public List<Integer> getIntegerList(String path) {
        List<Integer> out = new ArrayList<>();
        List<?> list = getList(path);
        if (list == null) {
            return out;
        }
        for (Object entry : list) {
            if (entry instanceof Number) {
                out.add(((Number) entry).intValue());
            }
        }
        return out;
    }

    @Override
    public List<Double> getDoubleList(String path) {
        List<Double> out = new ArrayList<>();
        List<?> list = getList(path);
        if (list == null) {
            return out;
        }
        for (Object entry : list) {
            if (entry instanceof Number) {
                out.add(((Number) entry).doubleValue());
            }
        }
        return out;
    }

    @Override
    public List<Boolean> getBooleanList(String path) {
        List<Boolean> out = new ArrayList<>();
        List<?> list = getList(path);
        if (list == null) {
            return out;
        }
        for (Object entry : list) {
            if (entry instanceof Boolean) {
                out.add((Boolean) entry);
            }
        }
        return out;
    }

    @Override
    public ConfigurationSection getConfigurationSection(String path) {
        Object value = get(path);
        return value instanceof ConfigurationSection ? (ConfigurationSection) value : null;
    }

    @Override
    public boolean isConfigurationSection(String path) {
        return get(path) instanceof ConfigurationSection;
    }

    @Override
    public void addDefault(String path, Object value) {
        Configuration configuration = getRoot();
        if (configuration == null) {
            return;
        }
        String full = fullPath.isEmpty() ? path : fullPath + separator() + path;
        if (configuration == this) {
            Map<String, Object> one = new LinkedHashMap<>();
            one.put(full, value);
            ((Configuration) this).addDefaults(one);
        } else {
            configuration.addDefault(full, value);
        }
    }

    /** The section as plain maps, which is what a serializer wants. */
    public Map<String, Object> getValues(boolean deep) {
        Map<String, Object> out = new LinkedHashMap<>();
        for (Map.Entry<String, Object> entry : map.entrySet()) {
            Object value = entry.getValue();
            if (deep && value instanceof MemorySection) {
                out.put(entry.getKey(), ((MemorySection) value).getValues(true));
            } else {
                out.put(entry.getKey(), value);
            }
        }
        return out;
    }

    @Override
    public String toString() {
        return getClass().getSimpleName() + "[path='" + fullPath + "']";
    }
}
