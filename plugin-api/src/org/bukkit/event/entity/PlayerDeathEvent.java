package org.bukkit.event.entity;
import org.bukkit.entity.Player;
import org.bukkit.entity.EntityType;
import org.bukkit.event.HandlerList;
public class PlayerDeathEvent extends EntityEvent {
 private String deathMessage;
 private final java.util.List<org.bukkit.inventory.ItemStack> drops = new java.util.ArrayList<>();
 private final java.util.List<org.bukkit.inventory.ItemStack> itemsToKeep = new java.util.ArrayList<>();
 private org.bukkit.damage.DamageSource damageSource;
 private boolean keepInventory;
 private int droppedExp;
 private boolean keepLevel;
 private int newExp;
 private int newTotalExp;
 private int newLevel;
 private double reviveHealth;
 private float deathSoundPitch = 1.0f;
 private org.bukkit.Sound deathSound;
 private boolean dropExperience = true;
 private org.bukkit.SoundCategory deathSoundCategory;
 private boolean playDeathSound = true;
 private float deathSoundVolume = 1.0f;
 private static final HandlerList HANDLERS = new HandlerList();
 public PlayerDeathEvent(Player player){this(player, null);}
 public PlayerDeathEvent(Player player, String deathMessage){super(player); this.deathMessage=deathMessage;}
 public PlayerDeathEvent(Player player, String deathMessage, boolean keepInventory){super(player); this.deathMessage=deathMessage; this.keepInventory=keepInventory;}
 @Override public Player getEntity(){return (Player) super.getEntity();}
 public Player getPlayer(){return getEntity();}
 public EntityType getEntityType(){return getEntity().getType();}
 public String getDeathMessage(){return deathMessage;}
 public void setDeathMessage(String message){deathMessage=message;}
 public java.util.List<org.bukkit.inventory.ItemStack> getDrops(){return drops;}
 public boolean getKeepInventory(){return keepInventory;}
 public void setKeepInventory(boolean keepInventory){this.keepInventory=keepInventory;}
 public int getDroppedExp(){return droppedExp;}
 public void setDroppedExp(int droppedExp){this.droppedExp=Math.max(0,droppedExp);}
 public boolean shouldKeepLevel(){return keepLevel;}
 public boolean getKeepLevel(){return keepLevel;}
 public java.util.List<org.bukkit.inventory.ItemStack> getItemsToKeep(){return itemsToKeep;}
 public org.bukkit.damage.DamageSource getDamageSource(){return damageSource;}
 public void setDamageSource(org.bukkit.damage.DamageSource value){damageSource=value;}
 public void setKeepLevel(boolean keepLevel){this.keepLevel=keepLevel;}
 public int getNewExp(){return newExp;}
 public void setNewExp(int value){newExp=Math.max(0,value);}
 public int getNewTotalExp(){return newTotalExp;}
 public void setNewTotalExp(int value){newTotalExp=Math.max(0,value);}
 public int getNewLevel(){return newLevel;}
 public void setNewLevel(int value){newLevel=Math.max(0,value);}
 public double getReviveHealth(){return reviveHealth;}
 public void setReviveHealth(double value){reviveHealth=Math.max(0.0,value);}
 public float getDeathSoundPitch(){return deathSoundPitch;}
 public void setDeathSoundPitch(float value){deathSoundPitch=value;}
 public org.bukkit.Sound getDeathSound(){return deathSound;}
 public void setDeathSound(org.bukkit.Sound value){deathSound=value;}
 public boolean shouldDropExperience(){return dropExperience;}
 public void setShouldDropExperience(boolean value){dropExperience=value;}
 public org.bukkit.SoundCategory getDeathSoundCategory(){return deathSoundCategory;}
 public void setDeathSoundCategory(org.bukkit.SoundCategory value){deathSoundCategory=value;}
 public boolean shouldPlayDeathSound(){return playDeathSound;}
 public void setShouldPlayDeathSound(boolean value){playDeathSound=value;}
 public float getDeathSoundVolume(){return deathSoundVolume;}
 public void setDeathSoundVolume(float value){deathSoundVolume=value;}
 public net.kyori.adventure.text.Component deathMessage(){return deathMessage == null ? null : net.kyori.adventure.text.Component.text(deathMessage);}
 public void deathMessage(net.kyori.adventure.text.Component value){deathMessage=value == null ? null : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(value);}
 public HandlerList getHandlers(){return HANDLERS;}
 public static HandlerList getHandlerList(){return HANDLERS;}
}
