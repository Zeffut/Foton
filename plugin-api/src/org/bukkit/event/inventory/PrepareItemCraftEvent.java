package org.bukkit.event.inventory;
import org.bukkit.inventory.CraftingInventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.InventoryView;
import org.bukkit.event.HandlerList;
public class PrepareItemCraftEvent extends InventoryEvent {
 private final CraftingInventory inventory; private final boolean repair; private static final HandlerList HANDLERS=new HandlerList();
 public PrepareItemCraftEvent(InventoryView view,CraftingInventory inventory,ItemStack result,boolean repair){super(view);this.inventory=inventory;this.repair=repair;if(result!=null)inventory.setResult(result);}
 @Override public CraftingInventory getInventory(){return inventory;} public boolean isRepair(){return repair;}
 public ItemStack getResult(){return inventory.getResult();} public void setResult(ItemStack result){inventory.setResult(result);}
 @Override public HandlerList getHandlers(){return HANDLERS;} public static HandlerList getHandlerList(){return HANDLERS;}
}
