package io.papermc.paper.command.brigadier.argument.resolvers;

import io.papermc.paper.command.brigadier.CommandSourceStack;
import io.papermc.paper.math.BlockPosition;

/** Resolves a block position relative to the command source location. */
public final class BlockPositionResolver {
    private final BlockPosition position;
    public BlockPositionResolver(BlockPosition position) { this.position = position; }
    public Object resolve(CommandSourceStack source) {
        if (position != null) return position;
        if (source == null || source.getLocation() == null) return null;
        return source.getLocation().toBlock();
    }
}
