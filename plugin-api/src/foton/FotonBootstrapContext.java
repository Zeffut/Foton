package foton;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginProviderContext;
import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager;
import java.nio.file.Path;
import org.bukkit.plugin.PluginDescriptionFile;
import net.kyori.adventure.text.logger.slf4j.ComponentLogger;

/** Immutable context shared by a Paper bootstrapper's two startup callbacks. */
public final class FotonBootstrapContext implements BootstrapContext, PluginProviderContext {
    private final PluginDescriptionFile metadata;
    private final Path dataDirectory;
    private final ComponentLogger logger;
    private final Path pluginSource;
    private final FotonLifecycleEventManager lifecycleManager;

    FotonBootstrapContext(
            PluginDescriptionFile metadata, Path dataDirectory, Path pluginSource) {
        this.metadata = metadata;
        this.dataDirectory = dataDirectory.toAbsolutePath().normalize();
        this.logger = ComponentLogger.logger(metadata.getName());
        this.pluginSource = pluginSource.toAbsolutePath().normalize();
        this.lifecycleManager = new FotonLifecycleEventManager(true);
    }

    @Override
    public PluginMeta getConfiguration() {
        return metadata;
    }

    @Override
    public PluginMeta getPluginMeta() {
        return metadata;
    }

    @Override
    public Path getDataDirectory() {
        return dataDirectory;
    }

    @Override
    public ComponentLogger getLogger() {
        return logger;
    }

    @Override
    public Path getPluginSource() {
        return pluginSource;
    }

    @Override
    public FotonLifecycleEventManager getLifecycleManager() {
        return lifecycleManager;
    }
}
