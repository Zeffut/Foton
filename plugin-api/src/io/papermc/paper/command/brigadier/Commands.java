package io.papermc.paper.command.brigadier;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.tree.LiteralCommandNode;
import io.papermc.paper.plugin.lifecycle.event.registrar.Registrar;
import java.util.Set;
import java.util.HashSet;

/** Brigadier registrar used by Paper lifecycle command handlers. */
public final class Commands implements Registrar {
    private final CommandDispatcher<CommandSourceStack> dispatcher = new CommandDispatcher<>();
    public CommandDispatcher<CommandSourceStack> getDispatcher() { return dispatcher; }
    public Set<LiteralCommandNode<CommandSourceStack>> register(LiteralCommandNode<CommandSourceStack> node) {
        dispatcher.getRoot().addChild(node);
        Set<LiteralCommandNode<CommandSourceStack>> result = new HashSet<>();
        result.add(node);
        return result;
    }
}
