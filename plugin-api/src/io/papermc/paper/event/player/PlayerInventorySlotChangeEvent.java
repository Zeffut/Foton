package io.papermc.paper.event.player;
import org.bukkit.entity.Player; import org.bukkit.inventory.ItemStack; import org.bukkit.event.Event; import org.bukkit.event.HandlerList;
public final class PlayerInventorySlotChangeEvent extends Event {
 private final Player player; private final int slot; private final ItemStack item; private static final HandlerList HANDLERS=new HandlerList();
 public PlayerInventorySlotChangeEvent(Player player,int slot,ItemStack item){this.player=player;this.slot=slot;this.item=item;} public Player getPlayer(){return player;} public int getSlot(){return slot;} public ItemStack getNewItemStack(){return item;}
 public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
