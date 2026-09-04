package org.bukkit.plugin.java;

import java.io.File;
import java.net.URL;
import java.net.URLClassLoader;
import org.bukkit.Server;
import org.bukkit.plugin.PluginDescriptionFile;

/**
 * Class loader associated with a JavaPlugin instance.
 *
 * <p>It carries the plugin's description because a plugin is allowed to call
 * {@link JavaPlugin#getName()}, {@link JavaPlugin#getLogger()} or
 * {@code getComponentLogger()} from its own constructor, and several do --
 * Geyser's Spigot bootstrap builds its logger there. The constructor runs
 * before the host can hand anything over, so the loader that is already
 * loading the class is what supplies it, which is the same route Bukkit uses.
 */
public class PluginClassLoader extends URLClassLoader {
    private JavaPlugin plugin;
    private Server server;
    private PluginDescriptionFile description;
    private File dataFolder;

    public PluginClassLoader(URL[] urls, ClassLoader parent) { super(urls, parent); }

    public JavaPlugin getPlugin() { return plugin; }

    public void setPlugin(JavaPlugin plugin) { this.plugin = plugin; }

    /** Records what a plugin loaded here needs before its constructor runs. */
    public void describe(Server server, PluginDescriptionFile description, File dataFolder) {
        this.server = server;
        this.description = description;
        this.dataFolder = dataFolder;
    }

    /**
     * Initializes {@code plugin} from what {@link #describe} recorded.
     *
     * <p>Does nothing when nothing was recorded, so a plugin constructed
     * outside a host -- a unit test, say -- still builds.
     */
    public void initialize(JavaPlugin plugin) {
        if (description == null) return;
        this.plugin = plugin;
        plugin.init(server, description, dataFolder);
    }
}
