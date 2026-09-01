package org.bukkit.event.player;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.event.block.Action;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.ItemStack;

public class PlayerInteractEvent extends PlayerEvent implements Cancellable {
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
    public org.bukkit.inventory.EquipmentSlot getHand() { return org.bukkit.inventory.EquipmentSlot.HAND; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
