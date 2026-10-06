package org.bukkit.event.entity;

import org.bukkit.entity.Entity;

/** Fired when one entity ignites another. */
public class EntityCombustByEntityEvent extends EntityCombustEvent {
    private final Entity combuster;
    public EntityCombustByEntityEvent(Entity combuster, Entity combustee, int duration) {
        this(combuster, combustee, (float) duration);
    }
    public EntityCombustByEntityEvent(Entity combuster, Entity combustee, float duration) {
        super(combustee, duration); this.combuster = combuster;
    }
    public Entity getCombuster() { return combuster; }
}
