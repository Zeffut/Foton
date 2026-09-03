package org.bukkit.plugin;

/** Logger associated with a Bukkit plugin. */
public class PluginLogger extends java.util.logging.Logger {
    private final Plugin plugin;
    public PluginLogger(Plugin plugin) {
        super(plugin == null ? "Plugin" : plugin.getName(), null);
        this.plugin = plugin;
    }
    public Plugin getPlugin() { return plugin; }
}
