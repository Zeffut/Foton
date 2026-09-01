package org.bukkit.event.entity;

import org.bukkit.entity.Entity;

/** Fired before one entity damages another. */
public final class EntityDamageByEntityEvent extends EntityDamageEvent {
    private final Entity damager;
    private final Entity entity;
    public EntityDamageByEntityEvent(Entity damager, Entity entity) { this(damager, entity, DamageCause.ENTITY_ATTACK); }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause) {
        super(entity, cause); this.damager = damager; this.entity = entity;
    }
    public Entity getDamager() { return damager; }
}
