package io.papermc.paper.plugin.bootstrap;

import org.bukkit.plugin.java.JavaPlugin;

/** Paper's bootstrap entry point, run before the plugin is constructed.
 *
 * <p>A plugin naming a `bootstrapper` in its `paper-plugin.yml` gets this
     * called first, which is where it registers lifecycle listeners. Libraries
     * are declared by the separate PluginLoader before this phase.
 */
public interface PluginBootstrap {
    /** Called before the plugin is constructed. */
    void bootstrap(BootstrapContext context);

    /** Builds the plugin instance.
     *
     * <p>Defaulted the way Paper defaults it: a plugin that does not override
     * this one is constructed by its no-argument constructor in the usual way,
     * using the bootstrapper's plugin class loader. */
    default JavaPlugin createPlugin(PluginProviderContext context) {
        try {
            return Class.forName(context.getConfiguration().getMainClass(), true,
                getClass().getClassLoader()).asSubclass(JavaPlugin.class)
                .getDeclaredConstructor().newInstance();
        } catch (ReflectiveOperationException error) {
            throw new IllegalStateException("Could not construct " + context.getConfiguration().getName(), error);
        }
    }
}
