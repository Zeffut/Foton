package org.bukkit.event.entity;

import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.Event;
import org.bukkit.event.HandlerList;

/** Base event for damage applied to an entity. */
public class EntityDamageEvent extends EntityEvent implements Cancellable {
    public enum DamageCause { ENTITY_ATTACK, PROJECTILE, SUFFOCATION, FALL, FIRE, FIRE_TICK, LAVA, DROWNING, BLOCK_EXPLOSION, ENTITY_EXPLOSION, CONTACT, MAGIC, POISON, LIGHTNING, VOID, SUICIDE, WITHER, THORNS, KILL, FLY_INTO_WALL, CUSTOM }
    private boolean cancelled;
    private final DamageCause cause;
    private org.bukkit.damage.DamageSource damageSource;
    private double damage;
    private static final HandlerList HANDLERS = new HandlerList();
    protected EntityDamageEvent(Entity entity) { this(entity, DamageCause.CUSTOM); }
    protected EntityDamageEvent(Entity entity, DamageCause cause) { super(entity); this.cause = cause == null ? DamageCause.CUSTOM : cause; this.damageSource = null; }
    /** Creates a damage event with its initial raw damage. */
    public EntityDamageEvent(Entity entity, DamageCause cause, double damage) { this(entity, cause); this.damage = damage; }
    public EntityDamageEvent(Entity entity, DamageCause cause, org.bukkit.damage.DamageSource source, double damage) {
        this(entity, cause);
        this.damageSource = source;
        this.damage = damage;
    }
    public org.bukkit.entity.EntityType getEntityType() { return getEntity() == null ? null : getEntity().getType(); }
    public DamageCause getCause() { return cause; }
    public org.bukkit.damage.DamageSource getDamageSource() { return damageSource; }
    public double getDamage() { return damage; }
    public void setDamage(double damage) { this.damage = damage; }
    /** Returns the post-modifier damage; Steel currently has no separate modifiers. */
    public double getFinalDamage() { return damage; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
