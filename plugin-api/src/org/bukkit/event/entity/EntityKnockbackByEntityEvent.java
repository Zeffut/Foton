package org.bukkit.event.entity;
import org.bukkit.entity.Entity;
import org.bukkit.entity.LivingEntity;
import org.bukkit.util.Vector;
@Deprecated
public class EntityKnockbackByEntityEvent extends EntityKnockbackEvent {
 private final Entity source;
 public EntityKnockbackByEntityEvent(LivingEntity entity,Entity source,KnockbackCause cause,double force,Vector raw,Vector knockback){super(entity,cause,force,raw,knockback);this.source=source;}
 public Entity getSourceEntity(){return source;}
}
