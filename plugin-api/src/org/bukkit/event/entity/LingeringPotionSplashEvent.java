package org.bukkit.event.entity;

import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.AreaEffectCloud;
import org.bukkit.entity.Entity;
import org.bukkit.entity.ThrownPotion;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;

/** Called when a lingering potion hits an area. */
public class LingeringPotionSplashEvent extends ProjectileHitEvent implements Cancellable {
    private static final HandlerList HANDLER_LIST = new HandlerList();
    private final AreaEffectCloud effectCloud;
    private boolean allowEmptyAreaEffectCreation;
    private boolean cancelled;

    @Deprecated
    public LingeringPotionSplashEvent(ThrownPotion potion, AreaEffectCloud effectCloud) {
        this(potion, null, null, null, effectCloud);
    }

    public LingeringPotionSplashEvent(ThrownPotion potion, Entity hitEntity, Block hitBlock,
                                      BlockFace hitFace, AreaEffectCloud effectCloud) {
        super(potion, hitEntity, hitBlock, hitFace);
        this.effectCloud = effectCloud;
    }

    @Override
    public ThrownPotion getEntity() {
        return (ThrownPotion) super.getEntity();
    }

    public AreaEffectCloud getAreaEffectCloud() { return effectCloud; }
    public void allowsEmptyCreation(boolean allow) { this.allowEmptyAreaEffectCreation = allow; }
    public boolean allowsEmptyCreation() { return allowEmptyAreaEffectCreation; }
    @Override public boolean isCancelled() { return cancelled; }
    @Override public void setCancelled(boolean cancel) { cancelled = cancel; }
    @Override public HandlerList getHandlers() { return HANDLER_LIST; }
    public static HandlerList getHandlerList() { return HANDLER_LIST; }
}
