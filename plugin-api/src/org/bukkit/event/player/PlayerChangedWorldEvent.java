package org.bukkit.event.player;

import org.bukkit.event.HandlerList;

import org.bukkit.entity.Player;

public class PlayerChangedWorldEvent extends PlayerEvent {
    private final org.bukkit.World from;

    public PlayerChangedWorldEvent(Player player, org.bukkit.World from) {
        super(player);
        this.from = from;
    }

    /** Kept for callers that predate the world argument. */
    public PlayerChangedWorldEvent(Player player) { this(player, null); }

    /** The world left behind. The one arrived in is on the player. */
    public org.bukkit.World getFrom() { return from; }

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
