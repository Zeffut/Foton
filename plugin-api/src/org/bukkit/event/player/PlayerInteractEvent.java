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
    private final org.bukkit.util.Vector clickedPosition;
    private boolean cancelled;
    private Result useItemInHand = Result.DEFAULT;
    private Result useInteractedBlock = Result.DEFAULT;
    private static final HandlerList HANDLERS = new HandlerList();

    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace) {
        this(player, action, item, clickedBlock, blockFace, null);
    }
    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace, org.bukkit.util.Vector clickedPosition) {
        super(player); this.action = action; this.item = item; this.clickedBlock = clickedBlock; this.blockFace = blockFace;
        this.clickedPosition = clickedPosition == null ? null : clickedPosition.clone();
    }
    public Action getAction() { return action; }
    public ItemStack getItem() { return item; }
    public boolean hasItem() { return item != null && !item.getType().isAir(); }
    public org.bukkit.Material getMaterial() { return item == null ? org.bukkit.Material.AIR : item.getType(); }
    public Block getClickedBlock() { return clickedBlock; }
    public BlockFace getBlockFace() { return blockFace; }
    public org.bukkit.util.Vector getClickedPosition() { return clickedPosition == null ? null : clickedPosition.clone(); }
    public org.bukkit.inventory.EquipmentSlot getHand() { return org.bukkit.inventory.EquipmentSlot.HAND; }
    public Result useItemInHand() { return useItemInHand; }
    public void setUseItemInHand(Result result) { useItemInHand = result == null ? Result.DEFAULT : result; }
    public Result useInteractedBlock() { return useInteractedBlock; }
    public void setUseInteractedBlock(Result result) { useInteractedBlock = result == null ? Result.DEFAULT : result; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
