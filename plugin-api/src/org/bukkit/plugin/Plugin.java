package org.bukkit.plugin;

public interface Plugin {
    java.io.File getDataFolder();
    PluginDescriptionFile getDescription();
    org.bukkit.Server getServer();
    java.util.logging.Logger getLogger();
    String getName();
    boolean isEnabled();
    void onEnable();
    void onDisable();
    default io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager getLifecycleManager() {
        return new io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager();
    }
    default PluginLoader getPluginLoader() {
        return plugin -> getServer().getPluginManager().disablePlugin(plugin);
    }
}
