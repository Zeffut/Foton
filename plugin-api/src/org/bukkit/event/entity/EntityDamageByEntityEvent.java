package org.bukkit.event.entity;

import com.google.common.base.Function;
import java.util.Map;
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
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause, double damage) {
        this(damager, entity, cause, null, damage);
    }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause,
            org.bukkit.damage.DamageSource source, double damage) {
        super(entity, cause, source, damage); this.damager = damager; this.critical = false;
    }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause,
            Map<DamageModifier, Double> modifiers, Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        this(damager, entity, cause, null, modifiers, modifierFunctions, false);
    }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause, org.bukkit.damage.DamageSource source,
            Map<DamageModifier, Double> modifiers, Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions) {
        this(damager, entity, cause, source, modifiers, modifierFunctions, false);
    }
    public EntityDamageByEntityEvent(Entity damager, Entity entity, DamageCause cause, org.bukkit.damage.DamageSource source,
            Map<DamageModifier, Double> modifiers, Map<DamageModifier, ? extends Function<? super Double, Double>> modifierFunctions,
            boolean critical) {
        super(entity, cause, source, modifiers, modifierFunctions);
        this.damager = damager;
        this.critical = critical;
    }
    public Entity getDamager() { return damager; }
    /** Whether the hit is a critical one: a falling, unsprinting, full-strength melee blow. */
    public boolean isCritical() { return critical; }
}
