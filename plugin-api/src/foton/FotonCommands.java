package foton;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.tree.LiteralCommandNode;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.builder.RequiredArgumentBuilder;
import com.mojang.brigadier.arguments.ArgumentType;
import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.command.brigadier.CommandRegistrationFlag;
import io.papermc.paper.plugin.lifecycle.event.registrar.Registrar;
import java.util.Set;
import java.util.HashSet;

/** Brigadier registrar used by Paper lifecycle command handlers. */
public final class FotonCommands implements Commands {
    public static LiteralArgumentBuilder<CommandSourceStack> literal(String name) {
        return LiteralArgumentBuilder.literal(name);
    }
    public static <T> RequiredArgumentBuilder<CommandSourceStack, T> argument(
            String name, ArgumentType<T> type) {
        return RequiredArgumentBuilder.argument(name, type);
    }
    private final CommandDispatcher<CommandSourceStack> dispatcher = new CommandDispatcher<>();
    public CommandDispatcher<CommandSourceStack> getDispatcher() { return dispatcher; }
    @SuppressWarnings("unchecked")
    public Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node) {
        LiteralCommandNode<CommandSourceStack> registered = node;

        // Paper's `CommandRegisteredEvent`, which exists so a listener can
        // *replace* the node -- wrapping it to add a permission check or a
        // suggestion provider is the usual reason. The event carries
        // `LiteralCommandNode<?>` because that is Paper's own signature, so a
        // listener can hand back a node built for a different source type; a
        // cast that fails costs that one replacement rather than the command.
        com.destroystokyo.paper.event.brigadier.CommandRegisteredEvent event =
                new com.destroystokyo.paper.event.brigadier.CommandRegisteredEvent(
                        node.getLiteral(), node);
        EventBridge.dispatch(event);
        LiteralCommandNode<?> replacement = event.getLiteral();
        if (replacement != null && replacement != node) {
            try {
                registered = (LiteralCommandNode<CommandSourceStack>) replacement;
            } catch (ClassCastException wrongSourceType) {
                System.out.println("[commands] a listener replaced /" + node.getLiteral()
                    + " with a node built for another source type; keeping the original");
            }
        }

        dispatcher.getRoot().addChild(registered);
        Set<LiteralCommandNode<CommandSourceStack>> result = new HashSet<>();
        result.add(registered);
        return result;
    }
    public Set<LiteralCommandNode<CommandSourceStack>> register(
            io.papermc.paper.plugin.configuration.PluginMeta meta,
            LiteralCommandNode<CommandSourceStack> node, String label,
            java.util.Collection<String> aliases) {
        return register(node);
    }
    public Set<LiteralCommandNode<CommandSourceStack>> registerWithFlags(
            io.papermc.paper.plugin.configuration.PluginMeta meta,
            LiteralCommandNode<CommandSourceStack> node, String label,
            java.util.Collection<String> aliases, java.util.Set<CommandRegistrationFlag> flags) {
        Set<LiteralCommandNode<CommandSourceStack>> result = register(node);
        if (flags != null && flags.contains(CommandRegistrationFlag.FLATTEN_ALIASES) && aliases != null) {
            for (String alias : aliases) {
                if (alias != null && !alias.isEmpty()) result.add(node);
            }
        }
        return result;
    }

    /** Executes a line against this plugin's registered Brigadier tree. */
    public boolean dispatch(org.bukkit.command.CommandSender sender, String line) {
        try {
            dispatcher.execute(line, new CommandSourceStack(sender,
                sender instanceof org.bukkit.entity.Entity entity ? entity.getLocation() : null));
            return true;
        } catch (com.mojang.brigadier.exceptions.CommandSyntaxException error) {
            sender.sendMessage(error.getMessage());
            return false;
        }
    }
}
