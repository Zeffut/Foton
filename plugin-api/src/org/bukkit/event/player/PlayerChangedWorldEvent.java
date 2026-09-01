package org.bukkit.event.player;

import org.bukkit.event.HandlerList;

import org.bukkit.entity.Player;

public class PlayerChangedWorldEvent extends PlayerEvent {
    public PlayerChangedWorldEvent(Player player) { super(player); }

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
