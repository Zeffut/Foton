package foton;

import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.plugin.java.JavaPlugin;

/** Dispatches the command lifecycle after a plugin has registered its handlers. */
public final class FotonLifecycle {
    private FotonLifecycle() {}
    public static void dispatchCommands(JavaPlugin plugin) {
        Commands commands = new Commands();
        ReloadableRegistrarEvent event = () -> commands;
        plugin.getLifecycleManager().dispatch(LifecycleEvents.COMMANDS, event);
        CommandMap.registerBrigadier(commands, plugin);
    }
}
