package org.bukkit.inventory;

import org.bukkit.block.Block;

/** An inventory held by a placed block, including menus without a block entity. */
public interface BlockInventoryHolder extends InventoryHolder {
    Block getBlock();
}
