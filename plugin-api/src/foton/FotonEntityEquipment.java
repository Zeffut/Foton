package foton;

import org.bukkit.inventory.EntityEquipment;
import org.bukkit.inventory.ItemStack;

/** Equipment view backed by a player's live inventory slots. */
final class FotonEntityEquipment implements EntityEquipment {
    private final FotonInventory inventory;
    FotonEntityEquipment(String owner) { inventory = new FotonInventory(owner); }
    @Override public ItemStack[] getArmorContents() { return inventory.getArmorContents(); }
    @Override public ItemStack getItemInMainHand() { return inventory.getItemInMainHand(); }
    @Override public ItemStack getItemInOffHand() { return inventory.getItemInOffHand(); }
}
