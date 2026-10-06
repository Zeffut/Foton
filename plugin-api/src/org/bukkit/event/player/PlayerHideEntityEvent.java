package org.bukkit.event.player;

import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.HandlerList;

/** An entity was hidden from a player. Fired after the change, only when it
 * actually changed what the player sees. */
public final class PlayerHideEntityEvent extends PlayerEvent {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Entity entity;

    public PlayerHideEntityEvent(Player player, Entity entity) {
        super(player);
        this.entity = entity;
    }

    public Entity getEntity() { return entity; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
