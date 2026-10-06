package org.bukkit.event.player;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.block.Action;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** A player used a hand: on a block, in the air, or with their feet.
 *
 * <p>Bukkit's semantics, kept exactly: the block and the item are decided
 * apart, an interaction with no block starts with its block use denied --
 * which is why such an event reads as cancelled before anyone touches it --
 * and cancelling denies both. */
public class PlayerInteractEvent extends PlayerEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Action action;
    private final ItemStack item;
    private final Block clickedBlock;
    private final BlockFace blockFace;
    private final EquipmentSlot hand;
    private final org.bukkit.util.Vector clickedPosition;
    private Result useItemInHand;
    private Result useInteractedBlock;

    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace) {
        this(player, action, item, clickedBlock, blockFace, EquipmentSlot.HAND);
    }
    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace,
            EquipmentSlot hand) {
        this(player, action, item, clickedBlock, blockFace, hand, null);
    }
    public PlayerInteractEvent(Player player, Action action, ItemStack item, Block clickedBlock, BlockFace blockFace,
            EquipmentSlot hand, org.bukkit.util.Vector clickedPosition) {
        super(player);
        this.action = action;
        this.item = item;
        this.clickedBlock = clickedBlock;
        this.blockFace = blockFace;
        this.hand = hand;
        this.clickedPosition = clickedPosition == null ? null : clickedPosition.clone();
        this.useItemInHand = Result.DEFAULT;
        this.useInteractedBlock = clickedBlock == null ? Result.DENY : Result.ALLOW;
    }
    public Action getAction() { return action; }
    public ItemStack getItem() { return item; }
    public boolean hasItem() { return item != null; }
    public boolean hasBlock() { return clickedBlock != null; }
    public boolean isBlockInHand() { return hasItem() && item.getType().isBlock(); }
    public org.bukkit.Material getMaterial() { return hasItem() ? item.getType() : org.bukkit.Material.AIR; }
    public Block getClickedBlock() { return clickedBlock; }
    public BlockFace getBlockFace() { return blockFace; }
    /** Where on the clicked block, relative to its corner; null when unknown. */
    public org.bukkit.util.Vector getClickedPosition() { return clickedPosition == null ? null : clickedPosition.clone(); }
    /** Paper's absolute form of {@link #getClickedPosition()}. */
    public org.bukkit.Location getInteractionPoint() {
        if (clickedBlock == null || clickedPosition == null) return null;
        return clickedBlock.getLocation().add(clickedPosition);
    }
    public EquipmentSlot getHand() { return hand; }
    public Result useItemInHand() { return useItemInHand; }
    public void setUseItemInHand(Result result) { useItemInHand = result; }
    public Result useInteractedBlock() { return useInteractedBlock; }
    public void setUseInteractedBlock(Result result) { useInteractedBlock = result; }
    @Deprecated @Override public boolean isCancelled() { return useInteractedBlock == Result.DENY; }
    @Override public void setCancelled(boolean cancel) {
        setUseInteractedBlock(cancel ? Result.DENY : useInteractedBlock == Result.DENY ? Result.DEFAULT : useInteractedBlock);
        setUseItemInHand(cancel ? Result.DENY : useItemInHand == Result.DENY ? Result.DEFAULT : useItemInHand);
    }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
