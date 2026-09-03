package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.inventory.HorseInventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

public class FotonHorseInventory implements HorseInventory, org.bukkit.inventory.LlamaInventory {
    private final String owner;
    public FotonHorseInventory(String owner) { this.owner = owner; }
    @Override public int getSize() { return 2; }
    @Override public ItemStack getItem(int slot) { return decode(Native.mountInventorySlot(owner, slot)); }
    @Override public void setItem(int slot, ItemStack item) { Native.setMountInventorySlot(owner, slot, encode(item)); }
    @Override public ItemStack getSaddle() { return getItem(0); }
    @Override public void setSaddle(ItemStack item) { setItem(0, item); }
    @Override public ItemStack getArmor() { return getItem(1); }
    @Override public void setArmor(ItemStack item) { setItem(1, item); }
    @Override public InventoryHolder getHolder() { try { return new FotonHorse(java.util.UUID.fromString(owner)); } catch (IllegalArgumentException ignored) { return null; } }
    @Override public ItemStack[] getContents() { return new ItemStack[] { getSaddle(), getArmor() }; }
    @Override public void setContents(ItemStack[] items) { setSaddle(items != null && items.length > 0 ? items[0] : null); setArmor(items != null && items.length > 1 ? items[1] : null); }
    @Override public void clear() { setContents(null); }
    @Override public void clear(int slot) { setItem(slot, null); }
    @Override public boolean contains(Material material) { return first(material) >= 0; }
    @Override public int first(Material material) { for (int i = 0; i < 2; i++) { ItemStack item = getItem(i); if (item != null && item.getType() == material) return i; } return -1; }
    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) { HashMap<Integer, ItemStack> left = new HashMap<>(); if (items != null) for (int i = 0; i < items.length; i++) { ItemStack item = items[i]; if (item != null && (item.getType() == Material.SADDLE || item.getType().name().endsWith("_ARMOR"))) { if (item.getType() == Material.SADDLE && getSaddle() == null) setSaddle(item); else if (getArmor() == null) setArmor(item); else left.put(i, item); } else if (item != null) left.put(i, item); } return left; }
    private static String encode(ItemStack item) { return item == null || item.getType().isAir() ? "" : item.getType().getKey().toString() + " " + item.getAmount(); }
    private static ItemStack decode(String value) { if (value == null || value.isBlank()) return null; String[] p = value.split(" "); Material m = Material.matchMaterial(p[0]); if (m == null) return null; ItemStack item = new ItemStack(m); if (p.length > 1) try { item.setAmount(Integer.parseInt(p[1])); } catch (NumberFormatException ignored) {} return item; }
}
