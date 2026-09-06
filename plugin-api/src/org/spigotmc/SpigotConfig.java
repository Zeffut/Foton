package org.spigotmc;

import org.bukkit.configuration.file.YamlConfiguration;

/** Spigot's server configuration, as plugins reach for it.
 *
 * <p>Plugins read {@code SpigotConfig.config} to check a setting Bukkit's own
 * API never exposed -- view distance overrides, mob spawn ranges, the timings
 * switch. Foton has no {@code spigot.yml}, so the configuration is empty rather
 * than absent: a plugin reading a key it does not find takes its own default,
 * which is the behaviour it has on a Spigot server that never set the key.
 * A null here would be a NullPointerException on their first read instead.
 */
public class SpigotConfig {
    /** Never null. Empty on Foton, because there is no spigot.yml to fill it. */
    public static YamlConfiguration config = new YamlConfiguration();

    /** The file the configuration came from, or null when there is none. */
    public static java.io.File CONFIG_FILE = null;

    /** Spigot's own accessor; kept so plugins that call it still link. */
    public static String getString(String path, String def) {
        return config.getString(path, def);
    }

    public static boolean getBoolean(String path, boolean def) {
        return config.getBoolean(path, def);
    }

    public static int getInt(String path, int def) {
        return config.getInt(path, def);
    }

    public static double getDouble(String path, double def) {
        return config.getDouble(path, def);
    }

    private SpigotConfig() { }
}
