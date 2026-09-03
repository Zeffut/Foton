package org.bukkit.event.entity;

import org.bukkit.entity.AbstractVillager;
import org.bukkit.event.HandlerList;
import org.bukkit.inventory.MerchantRecipe;

/** Fired when a villager acquires a new trade. */
public class VillagerAcquireTradeEvent extends EntityEvent {
    private final MerchantRecipe recipe;
    private static final HandlerList HANDLERS = new HandlerList();

    public VillagerAcquireTradeEvent(AbstractVillager entity, MerchantRecipe recipe) {
        super(entity);
        this.recipe = recipe;
    }

    @Override public AbstractVillager getEntity() { return (AbstractVillager) super.getEntity(); }
    public MerchantRecipe getRecipe() { return recipe; }
    @Override public HandlerList getHandlers() { return HANDLERS; }
    public static HandlerList getHandlerList() { return HANDLERS; }
}
