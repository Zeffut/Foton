package org.bukkit.plugin.java;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.StandardCopyOption;
import java.util.logging.Level;
import java.util.logging.Logger;
import org.bukkit.Server;
import org.bukkit.command.PluginCommand;
import org.bukkit.configuration.file.FileConfiguration;
import org.bukkit.configuration.file.YamlConfiguration;
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
    private FileConfiguration config;
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

    /** The plugin's own config.yml, read the first time it is asked for.
     *
     * Bukkit lays the jar's bundled config.yml underneath as defaults, so a
     * plugin that ships a new key in an update reads that key even from a
     * config file an operator wrote before the key existed. Skipping that step
     * is the difference between an update working and every new setting
     * silently reading as zero.
     */
    public FileConfiguration getConfig() {
        if (config == null) {
            reloadConfig();
        }
        return config;
    }

    public void reloadConfig() {
        config = YamlConfiguration.loadConfiguration(configFile());
        InputStream bundled = getResource("config.yml");
        if (bundled != null) {
            try (InputStreamReader reader =
                     new InputStreamReader(bundled, StandardCharsets.UTF_8)) {
                config.setDefaults(YamlConfiguration.loadConfiguration(reader));
            } catch (IOException error) {
                getLogger().log(Level.WARNING, "cannot read the bundled config.yml", error);
            }
        }
    }

    public void saveConfig() {
        if (config == null) {
            return;
        }
        try {
            config.save(configFile());
        } catch (IOException error) {
            getLogger().log(Level.SEVERE, "cannot write " + configFile(), error);
        }
    }

    /** Writes the jar's config.yml into the data folder, once. */
    public void saveDefaultConfig() {
        if (!configFile().exists()) {
            saveResource("config.yml", false);
        }
    }

    private File configFile() {
        return new File(getDataFolder(), "config.yml");
    }

    /** A file from inside the plugin's own jar. */
    public InputStream getResource(String name) {
        if (name == null) {
            return null;
        }
        return getClass().getClassLoader().getResourceAsStream(name);
    }

    /** Copies a file out of the jar into the data folder. */
    public void saveResource(String name, boolean replace) {
        InputStream source = getResource(name);
        if (source == null) {
            getLogger().warning("no resource named " + name + " in " + getName());
            return;
        }
        File target = new File(getDataFolder(), name.replace('\\', '/'));
        if (target.exists() && !replace) {
            return;
        }
        try (InputStream stream = source) {
            File parent = target.getParentFile();
            if (parent != null) {
                Files.createDirectories(parent.toPath());
            }
            Files.copy(stream, target.toPath(), StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException error) {
            getLogger().log(Level.SEVERE, "cannot write " + target, error);
        }
    }
}
