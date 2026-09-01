package org.bukkit.event.entity;

import org.bukkit.entity.Entity;

/** Fired before one entity damages another. */
public final class EntityDamageByEntityEvent extends EntityDamageEvent {
    private final Entity damager;
    private final Entity entity;
    public EntityDamageByEntityEvent(Entity damager, Entity entity) {
        super(entity); this.damager = damager; this.entity = entity;
    }
    public Entity getDamager() { return damager; }
}
