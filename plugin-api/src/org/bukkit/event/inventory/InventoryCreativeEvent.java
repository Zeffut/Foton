package org.bukkit.event.inventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.InventoryView;
public class InventoryCreativeEvent extends InventoryClickEvent {
 public InventoryCreativeEvent(InventoryView view, InventoryType.SlotType type, int slot, ItemStack newItem){super(view==null?null:null,newItem,null,ClickType.CREATIVE,slot,-1);}
 @Override public ItemStack getCursor(){return super.getCursor();}
 @Override public void setCancelled(boolean value){super.setCancelled(value);}
}
