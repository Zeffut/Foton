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
}
