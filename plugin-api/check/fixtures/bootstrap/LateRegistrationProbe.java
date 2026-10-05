package fixture;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.bootstrap.PluginProviderContext;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.plugin.java.JavaPlugin;

/** The same retained bootstrap context is used after each legal registration boundary. */
public final class LateRegistrationProbe extends JavaPlugin {
    private static BootstrapContext retained;
    private static void late(String phase) {
        for (boolean configured : new boolean[] {false, true}) {
            try {
                if (configured) retained.getLifecycleManager().registerEventHandler(
                    LifecycleEvents.COMMANDS.newHandler(event -> { throw new AssertionError("late callback"); }));
                else retained.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS,
                    event -> { throw new AssertionError("late callback"); });
                throw new AssertionError("late registration accepted in " + phase);
            } catch (IllegalStateException expected) {
                if (!"Cannot register lifecycle event handlers".equals(expected.getMessage())) throw expected;
                System.out.println("LATE_PROBE " + phase + ":" + configured + ":" + expected.getClass().getSimpleName()
                    + ":" + expected.getMessage());
            }
        }
    }
    @Override public void onLoad() { late("onLoad"); }
    @Override public void onEnable() { System.out.println("LATE_PROBE enabled"); }
    public static final class Bootstrap implements PluginBootstrap {
        @Override public void bootstrap(BootstrapContext context) {
            retained = context;
            context.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> late("commands"));
        }
        @Override public JavaPlugin createPlugin(PluginProviderContext context) {
            late("createPlugin");
            return new LateRegistrationProbe();
        }
    }
}
