package org.bukkit.event.block;

import org.bukkit.event.HandlerList;

import org.bukkit.block.Block;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;

public class BlockPlaceEvent extends BlockEvent implements Cancellable {
    private final Player player;
    private boolean cancelled;

    public BlockPlaceEvent(Block block, Player player) {
        super(block);
        this.player = player;
    }

    public Player getPlayer() { return player; }
    public Player getPlayerPlacing() { return player; }

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
