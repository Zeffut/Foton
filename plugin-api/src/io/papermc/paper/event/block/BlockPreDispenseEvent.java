package io.papermc.paper.event.block;

import org.bukkit.block.Block;
import org.bukkit.event.block.BlockDispenseEvent;
import org.bukkit.inventory.ItemStack;

/** Paper hook fired before Bukkit's BlockDispenseEvent. */
public class BlockPreDispenseEvent extends BlockDispenseEvent {
    private final int slot;
    public BlockPreDispenseEvent(Block block, int slot, ItemStack item) { super(block, item); this.slot = slot; }
    public int getSlot() { return slot; }
    public ItemStack getItemStack() { return getItem(); }
}
