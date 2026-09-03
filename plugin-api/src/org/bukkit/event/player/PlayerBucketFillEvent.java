package org.bukkit.event.player;

import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired before a player fills an empty bucket. */
public final class PlayerBucketFillEvent extends PlayerEvent implements Cancellable {
    private final Block block;
    private final Material bucket;
    private final org.bukkit.block.BlockFace blockFace;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerBucketFillEvent(Player player, Block block, Material bucket) { this(player, block, block, org.bukkit.block.BlockFace.SELF, bucket, null, null); }
    public PlayerBucketFillEvent(Player player, Block blockClicked, org.bukkit.block.BlockFace blockFace, Material bucket, org.bukkit.inventory.ItemStack itemInHand) { this(player, blockClicked, blockClicked, blockFace, bucket, itemInHand, null); }
    public PlayerBucketFillEvent(Player player, Block block, Block blockClicked, org.bukkit.block.BlockFace blockFace, Material bucket, org.bukkit.inventory.ItemStack itemInHand, org.bukkit.inventory.EquipmentSlot hand) { super(player); this.block = blockClicked; this.blockFace = blockFace; this.bucket = bucket; }
    public Block getBlockClicked() { return block; }
    public Block getBlock() { return block; }
    public Material getBucket() { return bucket; }
    public org.bukkit.block.BlockFace getBlockFace() { return blockFace; }
    public org.bukkit.inventory.ItemStack getItemStack() {
        return bucket == null ? null : new org.bukkit.inventory.ItemStack(bucket);
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
