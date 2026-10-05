package io.papermc.paper.plugin.bootstrap;

import java.nio.file.Path;

/** What a plugin is told about itself before it is constructed.
 *
 * <p>Paper's bootstrap phase runs before the plugin instance exists, so a
 * bootstrapper reads its own metadata and data directory from here rather than
 * from the plugin it has not built yet.
 */
public interface PluginProviderContext {
    io.papermc.paper.plugin.configuration.PluginMeta getConfiguration();

    /** Where the plugin may keep its files. */
    Path getDataDirectory();

    org.slf4j.Logger getLogger();

    /** The jar the plugin was loaded from. */
    Path getPluginSource();
}
