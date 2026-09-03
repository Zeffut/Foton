package io.papermc.paper.event.entity;

import java.util.Objects;
import org.bukkit.Location;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.entity.EntityEvent;

/** Paper movement event for living entities other than players. */
public class EntityMoveEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Location from;
    private final Location to;
    private boolean cancelled;

    public EntityMoveEvent(LivingEntity entity, Location from, Location to) {
        super(Objects.requireNonNull(entity, "entity"));
        this.from = Objects.requireNonNull(from, "from");
        this.to = Objects.requireNonNull(to, "to");
    }

    @Override public LivingEntity getEntity() { return (LivingEntity) super.getEntity(); }
    public Location getFrom() { return from; }
    public Location getTo() { return to; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
