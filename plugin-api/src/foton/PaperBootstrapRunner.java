package foton;

import java.io.File;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.plugin.java.JavaPlugin;
import org.bukkit.plugin.java.PluginClassLoader;

/** Owns pre-instance callbacks and their unpublished command tree. */
final class PaperBootstrapRunner {
    private final PluginBootstrap bootstrap;
    private final FotonBootstrapContext context;
    private final FotonCommands commands = new FotonCommands();

    PaperBootstrapRunner(File jar, PaperPluginDescriptor descriptor, PluginClassLoader loader)
            throws ReflectiveOperationException {
        context = new FotonBootstrapContext(descriptor,
            jar.toPath().toAbsolutePath().getParent().resolve(descriptor.getName()), jar.toPath());
        Class<?> type = Class.forName(descriptor.bootstrapper(), false, loader);
        if (!PluginBootstrap.class.isAssignableFrom(type))
            throw new IllegalArgumentException(type.getName() + " does not implement PluginBootstrap");
        bootstrap = type.asSubclass(PluginBootstrap.class).getDeclaredConstructor().newInstance();
    }

    void bootstrap() {
        try {
            bootstrap.bootstrap(context);
        } finally {
            context.getLifecycleManager().closeRegistration();
        }
    }

    void commands() {
        ReloadableRegistrarEvent<io.papermc.paper.command.brigadier.Commands> event = () -> commands;
        context.getLifecycleManager().dispatch(LifecycleEvents.COMMANDS, event);
    }

    JavaPlugin createPlugin() {
        return java.util.Objects.requireNonNull(bootstrap.createPlugin(context),
            "Paper PluginBootstrap.createPlugin returned null");
    }

    void seedCommands(FotonCommands destination) {
        CommandTreeSeed.copyInto(commands.getDispatcher().getRoot(), destination.getDispatcher().getRoot());
    }

}
