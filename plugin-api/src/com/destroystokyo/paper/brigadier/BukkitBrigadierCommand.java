package com.destroystokyo.paper.brigadier;

import com.mojang.brigadier.tree.LiteralCommandNode;

/** The Brigadier command node exposed by Paper command registration events. */
public final class BukkitBrigadierCommand {
    private final LiteralCommandNode<?> node;
    public BukkitBrigadierCommand(LiteralCommandNode<?> node) { this.node = node; }
    public LiteralCommandNode<?> getNode() { return node; }
}
