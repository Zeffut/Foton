package org.bukkit.event.block;
import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;
public class BlockDamageEvent extends Event implements Cancellable {
    private final Player player; private final Block block; private final org.bukkit.inventory.ItemStack item; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public BlockDamageEvent(Player player, Block block) { this(player, block, null); }
    public BlockDamageEvent(Player player, Block block, org.bukkit.inventory.ItemStack item) { this.player = player; this.block = block; this.item = item == null ? null : item.clone(); }
    public Player getPlayer() { return player; }
    public Block getBlock() { return block; }
    public org.bukkit.inventory.ItemStack getItemInHand() { return item == null ? null : item.clone(); }
    public boolean isCancelled() { return cancelled; }
    public void setCancelled(boolean value) { cancelled = value; }
    public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
