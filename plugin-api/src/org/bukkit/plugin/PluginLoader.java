package org.bukkit.plugin;

/** Loader lifecycle bridge exposed to legacy Bukkit plugins. */
@FunctionalInterface
public interface PluginLoader {
    void disablePlugin(Plugin plugin);
}
