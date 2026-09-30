package example;

import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.bootstrap.PluginProviderContext;
import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import org.bukkit.plugin.java.JavaPlugin;

public final class PaperBootstrap implements PluginBootstrap {
    private BootstrapContext bootstrapContext;
    private LifecycleEventManager bootstrapLifecycle;

    @Override
    public void bootstrap(BootstrapContext context) {
        PaperPlugin.steps.add("bootstrap");
        require(context instanceof PluginProviderContext,
            "bootstrap context did not expose provider metadata");
        require(context.getClass().getName().equals("foton.FotonBootstrapContext"),
            "bootstrap context was not Foton's concrete implementation");
        require(Modifier.isFinal(context.getClass().getModifiers()),
            "bootstrap context was not final");
        for (Field field : context.getClass().getDeclaredFields()) {
            if (!Modifier.isStatic(field.getModifiers())) {
                require(Modifier.isFinal(field.getModifiers()),
                    "bootstrap context field was mutable: " + field.getName());
            }
        }

        PluginProviderContext provider = (PluginProviderContext) context;
        require(provider.getConfiguration() == context.getPluginMeta(),
            "bootstrap metadata views were not identical");
        require(provider.getConfiguration().getName().equals("PaperFixture"),
            "bootstrap metadata had the wrong plugin name");
        require(provider.getDataDirectory().getFileName().toString().equals("PaperFixture"),
            "bootstrap data directory had the wrong name");
        require(provider.getLogger() != null,
            "bootstrap logger was missing");
        require(provider.getPluginSource().getFileName().toString().equals("PaperFixture.jar"),
            "bootstrap source had the wrong jar name");

        bootstrapContext = context;
        bootstrapLifecycle = context.getLifecycleManager();
        context.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
            PaperPlugin.steps.add("commands");
            Commands commands = (Commands) event.registrar();
            commands.register(Commands.literal("paperfixture").executes(command -> 1).build());
        });
    }

    @Override
    public JavaPlugin createPlugin(PluginProviderContext context) {
        PaperPlugin.steps.add("createPlugin");
        require(context == bootstrapContext,
            "bootstrap and createPlugin received different contexts");
        PaperPlugin plugin = new PaperPlugin("sentinel");
        require(plugin.getLifecycleManager() != bootstrapLifecycle,
            "bootstrap and plugin lifecycle managers were not dedicated");
        return plugin;
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
