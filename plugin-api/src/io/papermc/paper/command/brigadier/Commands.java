package io.papermc.paper.command.brigadier;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.tree.LiteralCommandNode;
import io.papermc.paper.plugin.lifecycle.event.registrar.Registrar;
import java.util.Collection;
import java.util.Set;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.builder.RequiredArgumentBuilder;

/** Paper command registrar interface. */
public interface Commands extends Registrar {
    static LiteralArgumentBuilder<CommandSourceStack> literal(String name) { return LiteralArgumentBuilder.literal(name); }
    static <T> RequiredArgumentBuilder<CommandSourceStack, T> argument(String name, com.mojang.brigadier.arguments.ArgumentType<T> type) { return RequiredArgumentBuilder.argument(name, type); }
    CommandDispatcher<CommandSourceStack> getDispatcher();
    Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node);
    default Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node, String label) { return register(node); }
    default Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node, String label, Collection<String> aliases) { return register(node); }
    Set<LiteralCommandNode<CommandSourceStack>> register(io.papermc.paper.plugin.configuration.PluginMeta meta, LiteralCommandNode<CommandSourceStack> node, String label, Collection<String> aliases);
    Set<LiteralCommandNode<CommandSourceStack>> registerWithFlags(io.papermc.paper.plugin.configuration.PluginMeta meta, LiteralCommandNode<CommandSourceStack> node, String label, Collection<String> aliases, Set<CommandRegistrationFlag> flags);
    boolean dispatch(org.bukkit.command.CommandSender sender, String line);
}
