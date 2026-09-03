package org.bukkit.command;

import org.bukkit.block.Block;

/** A command sender backed by a command block. */
public interface BlockCommandSender extends CommandSender {
    Block getBlock();
}
