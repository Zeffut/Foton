package org.bukkit.event.world;

import org.bukkit.GameEvent;
import org.bukkit.World;
import org.bukkit.entity.Entity;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Fired when a vanilla game event is emitted. */
public class GenericGameEvent extends Event {
    private final World world; private final Entity entity; private final GameEvent event; private final int radius;
    private static final HandlerList HANDLERS = new HandlerList();
    public GenericGameEvent(World world, Entity entity, GameEvent event, int radius) {
        this.world = world; this.entity = entity; this.event = event; this.radius = radius;
    }
    public World getWorld() { return world; }
    public Entity getEntity() { return entity; }
    public org.bukkit.Location getLocation() { return entity == null ? null : entity.getLocation(); }
    public GameEvent getEvent() { return event; }
    public int getRadius() { return radius; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
