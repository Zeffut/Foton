package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.event.Event;

public abstract class BlockEvent extends Event {
    protected final Block block;

    protected BlockEvent(Block block) { this.block = block; }

    public final Block getBlock() { return block; }
}
