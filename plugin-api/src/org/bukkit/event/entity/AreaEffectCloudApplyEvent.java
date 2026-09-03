package org.bukkit.event.entity;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.entity.AreaEffectCloud;
import org.bukkit.entity.LivingEntity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class AreaEffectCloudApplyEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLERS=new HandlerList(); private final List<LivingEntity> affected; private boolean cancelled;
 public AreaEffectCloudApplyEvent(AreaEffectCloud entity,List<LivingEntity> affected){super(entity);this.affected=affected==null?new ArrayList<>():affected;}
 public AreaEffectCloud getEntity(){return (AreaEffectCloud)super.getEntity();} public List<LivingEntity> getAffectedEntities(){return affected;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean cancel){cancelled=cancel;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
