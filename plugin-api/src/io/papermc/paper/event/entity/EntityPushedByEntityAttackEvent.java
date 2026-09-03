package io.papermc.paper.event.entity;

import java.util.Objects;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.event.entity.EntityEvent;

/** Fired when an entity is about to be pushed by another entity's attack. */
public class EntityPushedByEntityAttackEvent extends EntityEvent implements Cancellable {
    private static final HandlerList HANDLERS = new HandlerList();
    private final Entity pushedBy;
    private boolean cancelled;

    public EntityPushedByEntityAttackEvent(Entity pushed, Entity pushedBy) {
        super(Objects.requireNonNull(pushed, "pushed"));
        this.pushedBy = Objects.requireNonNull(pushedBy, "pushedBy");
    }

    public Entity getPushedBy() { return pushedBy; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
