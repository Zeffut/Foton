package org.bukkit.event.entity;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.EquipmentSlot;

/** A player placed an entity from an item: a boat, a minecart, an armor
 * stand, an end crystal. Cancelling takes it back and leaves the item. */
public class EntityPlaceEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Player player;
    private final Block block;
    private final BlockFace blockFace;
    private final EquipmentSlot hand;
    private boolean cancelled;

    public EntityPlaceEvent(Entity entity, Player player, Block block, BlockFace blockFace, EquipmentSlot hand) {
        super(entity);
        this.player = player;
        this.block = block;
        this.blockFace = blockFace;
        this.hand = hand;
    }

    @Deprecated
    public EntityPlaceEvent(Entity entity, Player player, Block block, BlockFace blockFace) {
        this(entity, player, block, blockFace, EquipmentSlot.HAND);
    }

    public Player getPlayer() { return player; }
    /** The block the entity was placed against. */
    public Block getBlock() { return block; }
    public BlockFace getBlockFace() { return blockFace; }
    public EquipmentSlot getHand() { return hand; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
