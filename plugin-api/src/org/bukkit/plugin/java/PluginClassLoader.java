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
    private final URLClassLoader libraries;
    private final ClassLoader dependencies;
    private JavaPlugin plugin;
    private Server server;
    private PluginDescriptionFile description;
    private File dataFolder;

    public PluginClassLoader(URL[] urls, ClassLoader parent) {
        super(urls.length == 0 ? urls : new URL[] {urls[0]}, PluginClassLoader.class.getClassLoader());
        dependencies = parent;
        libraries = new URLClassLoader(urls.length < 2 ? new URL[0] : java.util.Arrays.copyOfRange(urls, 1, urls.length),
            PluginClassLoader.class.getClassLoader());
    }

    @Override protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
        synchronized (getClassLoadingLock(name)) {
            try { return super.loadClass(name, resolve); }
            catch (ClassNotFoundException missingFromPlugin) {
                try { return libraries.loadClass(name); }
                catch (ClassNotFoundException missingFromLibraries) { return dependencies.loadClass(name); }
            }
        }
    }

    @Override public URL getResource(String name) {
        URL own = findResource(name);
        return own == null ? libraries.getResource(name) : own;
    }

    @Override public java.util.Enumeration<URL> getResources(String name) throws java.io.IOException {
        var resources = new java.util.LinkedHashSet<URL>();
        resources.addAll(java.util.Collections.list(findResources(name)));
        resources.addAll(java.util.Collections.list(libraries.getResources(name)));
        return java.util.Collections.enumeration(resources);
    }

    @Override public void close() throws java.io.IOException {
        try (libraries) { super.close(); }
    }

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
