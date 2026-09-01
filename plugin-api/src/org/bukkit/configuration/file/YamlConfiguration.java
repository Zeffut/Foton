package org.bukkit.configuration.file;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.configuration.ConfigurationSection;
import org.bukkit.configuration.InvalidConfigurationException;

/** The format every plugin's config.yml is written in.
 *
 * The reading and writing is `foton.Yaml`, which cannot name a Java class:
 * a config file is data, and a reader that can construct arbitrary types
 * turns reading a file into running code. A plugin's config directory is
 * exactly the sort of place that ends up writable by someone who should not
 * be running code.
 */
public class YamlConfiguration extends FileConfiguration {
    @Override
    public YamlConfigurationOptions options() {
        if (options == null) {
            options = new YamlConfigurationOptions(this);
        }
        return (YamlConfigurationOptions) options;
    }

    @Override
    public String saveToString() {
        return foton.Yaml.dump(getValues(true));
    }

    @Override
    public void loadFromString(String contents) throws InvalidConfigurationException {
        Object loaded;
        try {
            loaded = foton.Yaml.load(contents);
        } catch (RuntimeException error) {
            throw new InvalidConfigurationException(error);
        }
        map.clear();
        if (loaded instanceof Map) {
            adopt(this, asStringKeyed((Map<?, ?>) loaded));
        }
    }

    /** Turns nested maps into nested sections, which is what getters expect.
     *
     * A key holding a dot is split into levels rather than kept whole. That
     * looks wrong and is deliberate: Bukkit routes the same values through
     * `set`, so `worlds.world_nether: true` in a file is reachable as the path
     * `worlds.world_nether` and not as a key of that name. Plugins written
     * against that behavior depend on it, and a plugin that wants literal
     * dots changes `options().pathSeparator()`.
     */
    private static void adopt(ConfigurationSection into, Map<String, Object> values) {
        for (Map.Entry<String, Object> entry : values.entrySet()) {
            Object value = entry.getValue();
            if (value instanceof Map) {
                ConfigurationSection child = into.createSection(entry.getKey());
                adopt(child, asStringKeyed((Map<?, ?>) value));
            } else {
                into.set(entry.getKey(), value);
            }
        }
    }

    private static Map<String, Object> asStringKeyed(Map<?, ?> raw) {
        Map<String, Object> out = new LinkedHashMap<>();
        for (Map.Entry<?, ?> entry : raw.entrySet()) {
            out.put(String.valueOf(entry.getKey()), entry.getValue());
        }
        return out;
    }

    /** Loads a file, answering with an empty configuration if it cannot. */
    public static YamlConfiguration loadConfiguration(File file) {
        YamlConfiguration config = new YamlConfiguration();
        try {
            config.load(file);
        } catch (IOException | InvalidConfigurationException error) {
            // Bukkit logs and answers with an empty configuration rather than
            // throwing: a plugin calling this in onEnable would otherwise take
            // the server down over a stray tab character.
            System.out.println("[config] cannot read " + file + ": " + error);
        }
        return config;
    }

    public static YamlConfiguration loadConfiguration(Reader reader) {
        YamlConfiguration config = new YamlConfiguration();
        try {
            config.load(reader);
        } catch (IOException | InvalidConfigurationException error) {
            System.out.println("[config] cannot read: " + error);
        }
        return config;
    }

    public static YamlConfiguration loadConfiguration(InputStream stream) {
        return loadConfiguration(new InputStreamReader(stream, StandardCharsets.UTF_8));
    }
}
