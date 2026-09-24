package io.papermc.paper.plugin.loader;

import io.papermc.paper.plugin.bootstrap.PluginProviderContext;
import io.papermc.paper.plugin.loader.library.ClassPathLibrary;

/** Collects the isolated runtime libraries requested by a Paper plugin loader. */
public interface PluginClasspathBuilder {
    PluginClasspathBuilder addLibrary(ClassPathLibrary library);
    PluginProviderContext getContext();
}
