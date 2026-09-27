package foton;

import java.io.File;
import java.nio.file.Path;
import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.configuration.PluginMeta;
import io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.plugin.java.PluginClassLoader;

/** Owns pre-instance callbacks and their unpublished command tree. */
final class PaperBootstrapRunner {
    private final PluginBootstrap bootstrap;
    private final Context context;
    private final FotonCommands commands = new FotonCommands();

    PaperBootstrapRunner(File jar, PaperPluginDescriptor descriptor, PluginClassLoader loader)
            throws ReflectiveOperationException {
        context = new Context(jar.toPath().toAbsolutePath(), descriptor);
        bootstrap = Class.forName(descriptor.bootstrapper(), true, loader)
            .asSubclass(PluginBootstrap.class).getDeclaredConstructor().newInstance();
    }

    void bootstrap() {
        try {
            bootstrap.bootstrap(context);
        } finally {
            context.events.closeRegistration();
        }
    }

    void commands() {
        ReloadableRegistrarEvent<io.papermc.paper.command.brigadier.Commands> event = () -> commands;
        context.events.dispatch(LifecycleEvents.COMMANDS, event);
    }

    JavaPlugin createPlugin() {
        return java.util.Objects.requireNonNull(bootstrap.createPlugin(context),
            "Paper PluginBootstrap.createPlugin returned null");
    }

    void publishCommands(JavaPlugin plugin) { CommandMap.registerBrigadier(commands, plugin); }

    private static final class Context implements BootstrapContext {
        private final Path source;
        private final PaperPluginDescriptor descriptor;
        private final FotonLifecycleEventManager events = new FotonLifecycleEventManager(true);
        Context(Path source, PaperPluginDescriptor descriptor) {
            this.source = source;
            this.descriptor = descriptor;
        }
        @Override public PluginMeta getConfiguration() { return descriptor; }
        @Override public PluginMeta getPluginMeta() { return descriptor; }
        @Override public Path getDataDirectory() { return source.getParent().resolve(descriptor.getName()); }
        @Override public Path getPluginSource() { return source; }
        @Override public net.kyori.adventure.text.logger.slf4j.ComponentLogger getLogger() {
            return net.kyori.adventure.text.logger.slf4j.ComponentLogger.logger(descriptor.getName());
        }
        @Override public LifecycleEventManager getLifecycleManager() { return events; }
    }
}
