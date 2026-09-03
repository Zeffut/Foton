package org.bukkit.event.entity;

import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
import org.bukkit.potion.PotionEffect;

/** Fired when an active potion effect is added, changed, or removed. */
public class EntityPotionEffectEvent extends EntityEvent implements Cancellable {
    public enum Action { ADDED, CHANGED, REMOVED }

    /** Cause of a potion-effect transition, matching Bukkit/Paper semantics. */
    public enum Cause {
        AREA_EFFECT_CLOUD, ATTACK, BEACON, BLOCK, COMMAND, CONDUIT, CONVERSION,
        DOLPHIN, EXPIRATION, FOOD, ILLUSION, IMMERSION, PLUGIN, POTION_DRINK,
        POTION_SPLASH, RAID, REINFORCEMENT, SHIELD, SPIDER_SPAWN, TARDIGRADE,
        UNKNOWN, VILLAGER_TRADE, WITHER_ROSE
    }

    private final PotionEffect oldEffect;
    private final PotionEffect newEffect;
    private final Action action;
    private final Cause cause = Cause.PLUGIN;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();

    public EntityPotionEffectEvent(LivingEntity entity, PotionEffect oldEffect,
                                   PotionEffect newEffect, Action action) {
        super(entity);
        this.oldEffect = oldEffect;
        this.newEffect = newEffect;
        this.action = action;
    }

    @Override public LivingEntity getEntity() { return (LivingEntity) super.getEntity(); }
    public PotionEffect getOldEffect() { return oldEffect; }
    public PotionEffect getNewEffect() { return newEffect; }
    public Action getAction() { return action; }
    public Cause getCause() { return cause; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancelled) { this.cancelled = cancelled; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
