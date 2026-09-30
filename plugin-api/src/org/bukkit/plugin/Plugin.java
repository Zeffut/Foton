package org.bukkit.plugin;

/** A plugin. Paper parity: every plugin is its own commands' default
 * executor and tab completer, which is why a main class can simply override
 * {@code onCommand} without declaring an interface. */
public interface Plugin extends org.bukkit.command.TabExecutor {
    default java.io.InputStream getResource(String filename) { return null; }
    /** Loads config.yml from this plugin data folder. JavaPlugin overrides with a cached instance. */
    default org.bukkit.configuration.file.FileConfiguration getConfig() {
        java.io.File file = new java.io.File(getDataFolder(), "config.yml");
        return org.bukkit.configuration.file.YamlConfiguration.loadConfiguration(file);
    }
    java.io.File getDataFolder();
    /** Writes the in-memory config.yml back to the data folder. */
    void saveConfig();
    /** Discards the in-memory config.yml and reads it again from disk. */
    void reloadConfig();
    PluginDescriptionFile getDescription();
    org.bukkit.Server getServer();
    java.util.logging.Logger getLogger();
    /** Returns the Adventure component logger associated with this plugin. */
    default net.kyori.adventure.text.logger.slf4j.ComponentLogger getComponentLogger() {
        return net.kyori.adventure.text.logger.slf4j.ComponentLogger.logger(getName());
    }
    String getName();
    boolean isEnabled();
    /** Optional plugin-provided world generator; null selects the server generator. */
    default org.bukkit.generator.ChunkGenerator getDefaultWorldGenerator(String worldName, String id) { return null; }
    /** Paper-compatible metadata view; the loaded descriptor is already a PluginMeta. */
    default io.papermc.paper.plugin.configuration.PluginMeta getPluginMeta() {
        return getDescription();
    }
    /** Called once after construction and before enabling. */
    default void onLoad() {}
    void onEnable();
    void onDisable();
    default io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager getLifecycleManager() {
        return new io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager();
    }
    default PluginLoader getPluginLoader() {
        return plugin -> getServer().getPluginManager().disablePlugin(plugin);
    }
}
