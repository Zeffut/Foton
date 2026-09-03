package org.bukkit.event.block;

import org.bukkit.event.HandlerList;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;

public class BlockPlaceEvent extends BlockEvent implements Cancellable {
    private final Player player;
    private final org.bukkit.inventory.ItemStack item;
    private final org.bukkit.block.BlockState replacedState;
    private boolean cancelled;

    public BlockPlaceEvent(Block block, Player player) {
        this(block, player, null);
    }
    public BlockPlaceEvent(Block block, Player player, org.bukkit.inventory.ItemStack item) {
        super(block);
        this.player = player;
        this.item = item == null ? null : item.clone();
        this.replacedState = block == null ? null : block.getState();
    }

    public BlockPlaceEvent(Block block, org.bukkit.block.BlockState replacedState,
            Block blockAgainst, org.bukkit.inventory.ItemStack item, Player player,
            boolean canBuild) {
        this(block, replacedState, blockAgainst, item, player, canBuild, null);
    }

    public BlockPlaceEvent(Block block, org.bukkit.block.BlockState replacedState,
            Block blockAgainst, org.bukkit.inventory.ItemStack item, Player player,
            boolean canBuild, org.bukkit.inventory.EquipmentSlot hand) {
        super(block);
        this.player = player;
        this.item = item == null ? null : item.clone();
        this.replacedState = replacedState;
        this.cancelled = !canBuild;
    }

    public Player getPlayer() { return player; }
    public Player getPlayerPlacing() { return player; }
    /** The block whose placement this event describes. */
    public Block getBlockPlaced() { return getBlock(); }
    public Block getBlockAgainst() { return null; }
    public org.bukkit.inventory.ItemStack getItemInHand() { return item == null ? null : item.clone(); }
    public org.bukkit.block.BlockState getBlockReplacedState() { return replacedState; }

    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { this.cancelled = value; }

    /** Bukkit gives every event its own handler list, and plugins reach for
     * the static one to register or unregister by hand. Foton dispatches
     * through foton.EventBridge instead, so this is the shape rather than the
     * mechanism -- but a plugin that cannot find it does not compile. */
    private static final HandlerList HANDLERS = new HandlerList();

    @Override
    public HandlerList getHandlers() {
        return HANDLERS;
    }

    public static HandlerList getHandlerList() {
        return HANDLERS;
    }
}
