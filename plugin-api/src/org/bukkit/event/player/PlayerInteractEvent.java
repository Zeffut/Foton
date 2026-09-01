package org.bukkit.event.player;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

public class PlayerInteractEvent extends PlayerEvent implements Cancellable {
    public enum Action { LEFT_CLICK_BLOCK, RIGHT_CLICK_BLOCK, LEFT_CLICK_AIR, RIGHT_CLICK_AIR, PHYSICAL }
    private final Action action;
    private final ItemStack item;
    private final Block clickedBlock;
    private final BlockFace blockFace;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace) {
        super(player); this.action = action; this.item = item; this.clickedBlock = clickedBlock; this.blockFace = blockFace;
    }
    public Action getAction() { return action; }
    public ItemStack getItem() { return item; }
    public Block getClickedBlock() { return clickedBlock; }
    public BlockFace getBlockFace() { return blockFace; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
