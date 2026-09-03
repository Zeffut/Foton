package org.bukkit.event.player;
import org.bukkit.entity.Entity;
import org.bukkit.entity.Player;
import org.bukkit.event.entity.EntityUnleashEvent;
import org.bukkit.inventory.EquipmentSlot;
public class PlayerUnleashEntityEvent extends EntityUnleashEvent {
 private final Player player; private final EquipmentSlot hand; private boolean cancelled;
 public PlayerUnleashEntityEvent(Entity entity, Player player, EquipmentSlot hand, boolean dropLeash){super(entity,UnleashReason.PLAYER_UNLEASH,dropLeash);this.player=player;this.hand=hand;}
 public PlayerUnleashEntityEvent(Entity entity, Player player, EquipmentSlot hand){this(entity,player,hand,false);} public PlayerUnleashEntityEvent(Entity entity,Player player){this(entity,player,EquipmentSlot.HAND);}
 public Player getPlayer(){return player;} public EquipmentSlot getHand(){return hand;} @Override public boolean isCancelled(){return cancelled;} @Override public void setCancelled(boolean value){cancelled=value;}
}
