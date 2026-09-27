package fixture;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.bootstrap.PluginProviderContext;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import io.papermc.paper.command.brigadier.Commands;
import org.bukkit.plugin.java.JavaPlugin;

/** Compiled against the pinned Paper API and run unchanged on both hosts. */
public final class BootstrapProbe extends JavaPlugin {
    public BootstrapProbe(String value) { trace("construct:" + value); }
    @Override public void onLoad() { trace("load"); }
    @Override public void onEnable() { trace("enable"); }
    @Override public void onDisable() { trace("disable"); }
    private static void trace(String phase) { System.out.println("BOOTSTRAP_PROBE " + phase); }

    public static final class Loader implements io.papermc.paper.plugin.loader.PluginLoader {
        @Override public void classloader(io.papermc.paper.plugin.loader.PluginClasspathBuilder builder) {
            trace("loader");
            builder.addLibrary(new io.papermc.paper.plugin.loader.library.impl.JarLibrary(
                java.nio.file.Path.of(System.getProperty("foton.bootstrap.library"))));
        }
    }

    public static final class Bootstrap implements PluginBootstrap {
        @Override public void bootstrap(BootstrapContext context) {
            trace("bootstrap:" + context.getConfiguration().getName());
            try {
                trace("library:" + Class.forName("fixturelibrary.Marker").getMethod("value").invoke(null));
            } catch (ReflectiveOperationException error) { throw new IllegalStateException(error); }
            context.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
                trace("commands");
                event.registrar().register(Commands.literal("bootstrap_probe")
                    .executes(command -> { trace("execute"); return 1; }).build());
            });
        }
        @Override public JavaPlugin createPlugin(PluginProviderContext context) {
            trace("create");
            return new BootstrapProbe("custom");
        }
    }
}
