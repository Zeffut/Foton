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
    public Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node) {
        dispatcher.getRoot().addChild(node);
        Set<LiteralCommandNode<CommandSourceStack>> result = new HashSet<>();
        result.add(node);
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
