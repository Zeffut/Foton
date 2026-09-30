package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

/** A block with an inventory starts working on an item. */
public class InventoryBlockStartEvent extends BlockEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    protected ItemStack source;

    public InventoryBlockStartEvent(Block block, ItemStack source) {
        super(block);
        this.source = source;
    }

    public ItemStack getSource() { return source; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
