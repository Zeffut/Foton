package org.bukkit.event.block;

import org.bukkit.block.Block;
import org.bukkit.inventory.ItemStack;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired immediately before a dispenser or dropper dispenses an item. */
public class BlockDispenseEvent extends Event implements Cancellable {
    private final Block block;
    private ItemStack item;
    private org.bukkit.util.Vector velocity;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockDispenseEvent(Block block, ItemStack item) { this(block, item, new org.bukkit.util.Vector()); }
    public BlockDispenseEvent(Block block, ItemStack item, org.bukkit.util.Vector velocity) { this.block = block; this.item = item; this.velocity = velocity == null ? new org.bukkit.util.Vector() : velocity.clone(); }
    public Block getBlock() { return block; }
    public ItemStack getItem() { return item; }
    public void setItem(ItemStack item) { this.item = item; }
    public org.bukkit.util.Vector getVelocity() { return velocity.clone(); }
    public void setVelocity(org.bukkit.util.Vector value) { velocity = value == null ? new org.bukkit.util.Vector() : value.clone(); }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
