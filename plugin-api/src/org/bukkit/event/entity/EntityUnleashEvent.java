package org.bukkit.event.entity;
import org.bukkit.entity.Entity;
import org.bukkit.event.Cancellable;
import org.bukkit.event.HandlerList;
public class EntityUnleashEvent extends EntityEvent implements Cancellable {
 private static final HandlerList HANDLER_LIST=new HandlerList(); private final UnleashReason reason; private boolean dropLeash, cancelled;
 public EntityUnleashEvent(Entity entity, UnleashReason reason){this(entity,reason,false);} public EntityUnleashEvent(Entity entity,UnleashReason reason,boolean dropLeash){super(entity);this.reason=reason;this.dropLeash=dropLeash;}
 public UnleashReason getReason(){return reason;} public boolean isDropLeash(){return dropLeash;} public void setDropLeash(boolean value){dropLeash=value;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;} public HandlerList getHandlers(){return HANDLER_LIST;} public static HandlerList getHandlerList(){return HANDLER_LIST;}
 public enum UnleashReason { HOLDER_GONE, PLAYER_UNLEASH, DISTANCE, LEASHED_GONE, UNKNOWN }
}
