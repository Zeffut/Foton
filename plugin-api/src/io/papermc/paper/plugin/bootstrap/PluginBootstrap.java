package io.papermc.paper.plugin.bootstrap;

import org.bukkit.plugin.java.JavaPlugin;

/** Paper's bootstrap entry point, run before the plugin is constructed.
 *
 * <p>A plugin naming a `bootstrapper` in its `paper-plugin.yml` gets this
 * called first, which is where it registers lifecycle listeners and declares
 * the libraries it needs. The class has to exist for such a plugin to load at
 * all, whether or not Foton runs the phase.
 */
public interface PluginBootstrap {
    /** Called before the plugin is constructed. */
    void bootstrap(BootstrapContext context);

    /** Builds the plugin instance.
     *
     * <p>Defaulted the way Paper defaults it: a plugin that does not override
     * this one is constructed by its no-argument constructor in the usual way,
     * and returning null is how a bootstrapper says so. */
    default JavaPlugin createPlugin(PluginProviderContext context) {
        return null;
    }
}
