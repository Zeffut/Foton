package io.papermc.paper.event.player;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

/** Fired when a player places or removes an item in a flower pot. */
public class PlayerFlowerPotManipulateEvent extends org.bukkit.event.player.PlayerEvent implements Cancellable {
    private final Block flowerpot; private final ItemStack item; private final boolean placing; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerFlowerPotManipulateEvent(Player player, Block flowerpot, ItemStack item, boolean placing) { super(player); this.flowerpot = flowerpot; this.item = item; this.placing = placing; }
    public Block getFlowerpot() { return flowerpot; }
    public ItemStack getItem() { return item; }
    public boolean isPlacing() { return placing; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
