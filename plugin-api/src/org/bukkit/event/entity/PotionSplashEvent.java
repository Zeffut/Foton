package org.bukkit.event.entity;

import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.entity.LivingEntity;
import org.bukkit.entity.ThrownPotion;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Fired when a thrown potion applies splash effects. */
public class PotionSplashEvent extends EntityEvent implements Cancellable {
    private final Map<LivingEntity, Double> affected;
    private boolean cancelled;
    private static final HandlerList HANDLERS = new HandlerList();
    public PotionSplashEvent(ThrownPotion potion, Map<LivingEntity, Double> affected) {
        super(potion);
        this.affected = affected == null ? new LinkedHashMap<>() : new LinkedHashMap<>(affected);
    }
    @Override public ThrownPotion getEntity() { return (ThrownPotion) super.getEntity(); }
    public ThrownPotion getPotion() { return getEntity(); }
    public Collection<LivingEntity> getAffectedEntities() { return Collections.unmodifiableSet(affected.keySet()); }
    public double getIntensity(LivingEntity entity) { return affected.getOrDefault(entity, 0.0); }
    public void setIntensity(LivingEntity entity, double intensity) {
        if (entity != null) affected.put(entity, Math.max(0.0, intensity));
    }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean value) { cancelled = value; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
