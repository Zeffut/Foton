package org.bukkit.plugin.java;

import java.io.File;
import java.util.logging.Logger;
import org.bukkit.Server;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;

/** What a plugin extends.
 *
 * Bukkit's own version reads its context out of the PluginClassLoader that is
 * loading it; this one is handed the same context by `init` immediately after
 * construction, which the host controls and a plugin cannot observe.
 */
public abstract class JavaPlugin implements Plugin {
    private Server server;
    private PluginDescriptionFile description;
    private File dataFolder;
    private Logger logger;
    private boolean enabled;
    private final java.util.Map<String, PluginCommand> commands = new java.util.HashMap<>();

    public JavaPlugin() {}

    public final void init(
        Server server, PluginDescriptionFile description, File dataFolder, String[] commandNames) {
        this.server = server;
        this.description = description;
        this.dataFolder = dataFolder;
        this.logger = Logger.getLogger(description.getName());
        for (String name : commandNames) {
            commands.put(name.toLowerCase(java.util.Locale.ROOT), new PluginCommand(name, this));
        }
    }

    @Override public File getDataFolder() { return dataFolder; }
    @Override public PluginDescriptionFile getDescription() { return description; }
    @Override public Server getServer() { return server; }
    @Override public Logger getLogger() { return logger; }
    @Override public String getName() { return description.getName(); }
    @Override public boolean isEnabled() { return enabled; }

    public final void setEnabled(boolean value) { this.enabled = value; }

    public PluginCommand getCommand(String name) {
        return commands.get(name.toLowerCase(java.util.Locale.ROOT));
    }

    @Override public void onEnable() {}
    @Override public void onDisable() {}

    public void saveDefaultConfig() {}
    public void reloadConfig() {}
}
