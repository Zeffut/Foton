package org.bukkit.event.entity;

import org.bukkit.entity.Entity;

/** Fired before one entity damages another. */
public class EntityDamageByEntityEvent extends EntityDamageEvent {
    private final Entity damager;
    private final boolean critical;
    public EntityDamageByEntityEvent(Entity damager, Entity entity) { this(damager, entity, DamageCause.ENTITY_ATTACK); }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause) {
        this(damager, entity, cause, false);
    }
    /** An attack that vanilla judged critical, or not. */
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause, boolean critical) {
        super(entity, cause); this.damager = damager; this.critical = critical;
    }
    /** Creates an entity-damage event with an initial raw damage value. */
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause, double damage) {
        super(entity, cause, damage); this.damager = damager; this.critical = false;
    }
    public Entity getDamager() { return damager; }
    /** Whether the hit is a critical one: a falling, unsprinting, full-strength melee blow. */
    public boolean isCritical() { return critical; }
}
