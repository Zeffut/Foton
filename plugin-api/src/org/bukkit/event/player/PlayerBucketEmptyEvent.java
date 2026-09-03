package org.bukkit.event.player;

import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

public class PlayerBucketEmptyEvent extends PlayerEvent implements Cancellable {
    private final Block block; private final Material bucket; private final org.bukkit.block.BlockFace blockFace; private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PlayerBucketEmptyEvent(Player player, Material bucket) { this(player, null, bucket); }
    public PlayerBucketEmptyEvent(Player player, Block block, Material bucket) { this(player, block, org.bukkit.block.BlockFace.SELF, bucket, null, null); }
    public PlayerBucketEmptyEvent(Player player, Block block, org.bukkit.block.BlockFace face, Material bucket, org.bukkit.inventory.ItemStack item) { this(player, block, face, bucket, item, null); }
    public PlayerBucketEmptyEvent(Player player, Block block, org.bukkit.block.BlockFace face, Material bucket, org.bukkit.inventory.ItemStack item, org.bukkit.inventory.EquipmentSlot hand) { super(player); this.block = block; this.blockFace = face; this.bucket = bucket; }
    public Block getBlockClicked() { return block; }
    /** Bukkit-compatible alias for the clicked block. */
    public Block getBlock() { return block; }
    public Material getBucket() { return bucket; }
    public org.bukkit.block.BlockFace getBlockFace() { return blockFace; }
    /** Returns the bucket stack consumed by this empty action. */
    public org.bukkit.inventory.ItemStack getItemStack() {
        return bucket == null ? null : new org.bukkit.inventory.ItemStack(bucket);
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
