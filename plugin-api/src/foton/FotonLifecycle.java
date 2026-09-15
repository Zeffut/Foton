package foton;

import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager;
import io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;
import org.bukkit.command.PluginCommand;
import org.bukkit.plugin.java.JavaPlugin;

/** Dispatches the command lifecycle after a plugin has registered its handlers. */
public final class FotonLifecycle {
    private FotonLifecycle() {}

    public static void transfer(
            FotonBootstrapContext context, JavaPlugin plugin) {
        FotonLifecycleEventManager bootstrap = context.getLifecycleManager();
        bootstrap.transferTo(plugin.getLifecycleManager());
    }

    public static void dispatchCommands(JavaPlugin plugin) {
        Commands commands = new FotonCommands();
        ReloadableRegistrarEvent event = () -> commands;
        try {
            plugin.getLifecycleManager().dispatch(LifecycleEvents.COMMANDS, event);
        } catch (LinkageError unsupportedLifecycleShape) {
            System.out.println("[host] " + plugin.getName() + ": Paper lifecycle handler shape is unavailable; continuing without lifecycle commands");
        }
        for (var node : commands.getDispatcher().getRoot().getChildren()) {
            if (!(node instanceof com.mojang.brigadier.tree.LiteralCommandNode<?> literal)) {
                continue;
            }
            PluginCommand command = new PluginCommand(literal.getLiteral(), plugin);
            command.setExecutor((sender, ignored, label, args) -> commands.dispatch(
                sender, args.length == 0 ? label : label + " " + String.join(" ", args)));
            CommandMap.register(command);
        }
        CommandMap.registerBrigadier(commands, plugin);
    }
}
