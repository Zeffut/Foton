package foton;

import com.mojang.brigadier.tree.CommandNode;
import com.mojang.brigadier.tree.RootCommandNode;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;

/** Copies staged bootstrap nodes without sharing mutable child maps between enables. */
final class CommandTreeSeed {
    private CommandTreeSeed() {}
    static <S> void copyInto(CommandNode<S> source, CommandNode<S> target) {
        Map<CommandNode<S>, CommandNode<S>> copies = new IdentityHashMap<>();
        copies.put(source, target);
        List<CommandNode<S>> nodes = new ArrayList<>();
        collect(source, new IdentityHashMap<>(), nodes);
        for (CommandNode<S> node : nodes) shell(node, copies);
        // Build redirects first, then children: root/ancestor redirects are legal.
        for (CommandNode<S> node : nodes)
            for (CommandNode<S> child : node.getChildren())
                copies.get(node).addChild(copies.get(child));
    }

    private static <S> void collect(CommandNode<S> node,
            Map<CommandNode<S>, Boolean> seen, List<CommandNode<S>> nodes) {
        if (seen.put(node, Boolean.TRUE) != null) return;
        nodes.add(node);
        for (CommandNode<S> child : node.getChildren()) collect(child, seen, nodes);
        if (node.getRedirect() != null) collect(node.getRedirect(), seen, nodes);
    }

    private static <S> CommandNode<S> shell(CommandNode<S> node,
            Map<CommandNode<S>, CommandNode<S>> copies) {
        CommandNode<S> copy = copies.get(node);
        if (copy != null) return copy;
        if (node instanceof RootCommandNode<S>) copy = new RootCommandNode<>();
        else {
            var builder = node.createBuilder();
            if (node.getRedirect() != null)
                builder.forward(shell(node.getRedirect(), copies), node.getRedirectModifier(), node.isFork());
            copy = builder.build();
        }
        copies.put(node, copy);
        return copy;
    }
}
