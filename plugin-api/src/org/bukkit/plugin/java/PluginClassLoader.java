package org.bukkit.plugin.java;

import java.net.URL;
import java.net.URLClassLoader;

/** Class loader associated with a JavaPlugin instance. */
public class PluginClassLoader extends URLClassLoader {
    private JavaPlugin plugin;
    public PluginClassLoader(URL[] urls, ClassLoader parent) { super(urls, parent); }
    public JavaPlugin getPlugin() { return plugin; }
    public void setPlugin(JavaPlugin plugin) { this.plugin = plugin; }
}
