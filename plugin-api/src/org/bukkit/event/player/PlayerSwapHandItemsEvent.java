package org.bukkit.event.player;
import org.bukkit.entity.Player; import org.bukkit.inventory.ItemStack; import org.bukkit.event.Cancellable; import org.bukkit.event.HandlerList;
public final class PlayerSwapHandItemsEvent extends PlayerEvent implements Cancellable {
 private ItemStack mainHand,offHand; private boolean cancelled; private static final HandlerList HANDLERS=new HandlerList();
 public PlayerSwapHandItemsEvent(Player player,ItemStack mainHand,ItemStack offHand){super(player);this.mainHand=mainHand;this.offHand=offHand;} public ItemStack getMainHandItem(){return mainHand;} public void setMainHandItem(ItemStack value){mainHand=value;} public ItemStack getOffHandItem(){return offHand;} public void setOffHandItem(ItemStack value){offHand=value;}
 public boolean isCancelled(){return cancelled;} public void setCancelled(boolean value){cancelled=value;} public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
