package org.bukkit.inventory.meta;
import java.util.ArrayList;
import java.util.List;
import org.bukkit.inventory.ItemStack;
public class SimpleCrossbowMeta extends SimpleItemMeta implements CrossbowMeta {
 private List<ItemStack> projectiles=new ArrayList<>();
 public boolean hasChargedProjectiles(){return !projectiles.isEmpty();}
 public List<ItemStack> getChargedProjectiles(){List<ItemStack> out=new ArrayList<>(); for(ItemStack s:projectiles) out.add(s==null?null:s.clone()); return out;}
 public void setChargedProjectiles(List<ItemStack> values){projectiles=new ArrayList<>(); if(values!=null) for(ItemStack s:values) if(s!=null) projectiles.add(s.clone());}
 public void addChargedProjectile(ItemStack s){if(s!=null) projectiles.add(s.clone());}
 @Override public SimpleCrossbowMeta clone(){SimpleCrossbowMeta c=(SimpleCrossbowMeta)super.clone(); c.setChargedProjectiles(projectiles); return c;}
}
