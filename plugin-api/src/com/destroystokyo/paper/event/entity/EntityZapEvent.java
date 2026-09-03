package com.destroystokyo.paper.event.entity;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LightningStrike;
import org.bukkit.entity.EntityType;
import org.bukkit.event.HandlerList;
import org.bukkit.event.entity.EntityTransformEvent;
public class EntityZapEvent extends EntityTransformEvent {
 private static final HandlerList HANDLERS=new HandlerList(); private final LightningStrike bolt; private final Entity replacement;
 public EntityZapEvent(Entity entity, LightningStrike bolt, Entity replacement){super(entity,replacement);this.bolt=bolt;this.replacement=replacement;}
 public LightningStrike getBolt(){return bolt;} public EntityType getEntityType(){return getEntity().getType();} public Entity getReplacementEntity(){return replacement;}
 @Override public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
