package io.papermc.paper.plugin.loader;

/** Configures a Paper plugin's runtime class path before bootstrap. */
public interface PluginLoader {
    void classloader(PluginClasspathBuilder classpathBuilder);
}
