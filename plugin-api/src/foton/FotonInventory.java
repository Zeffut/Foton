package foton;

import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;

/** A player's inventory, read and written across JNI.
 *
 * Every slot is a separate crossing, which is slower than one call for the
 * whole thing and is what keeps the contract honest: nothing here holds a
 * reference into the live inventory, so a plugin cannot change a chest from
 * its own thread while the tick is reading it.
 *
 * A slot crosses as a string -- `minecraft:diamond_sword 3` -- rather than as
 * an object. Building a Java object from Rust means calling a constructor by
 * signature, and a signature that drifts is a NoSuchMethodError at the worst
 * possible moment; a string that fails to parse is an empty slot.
 */
public final class FotonInventory implements PlayerInventory {
    /** Vanilla's player inventory: 36 storage slots, 4 armor, 1 offhand. */
    private static final int SIZE = 41;
    private static final int ARMOR = 36;
    private static final int OFFHAND = 40;

    private final String owner;

    FotonInventory(String owner) {
        this.owner = owner;
    }

    @Override
    public int getSize() {
        return SIZE;
    }

    @Override
    public ItemStack getItem(int slot) {
        return decode(Native.inventorySlot(owner, slot));
    }

    @Override
    public void setItem(int slot, ItemStack item) {
        Native.setInventorySlot(owner, slot, encode(item));
    }

    @Override
    public ItemStack[] getContents() {
        ItemStack[] contents = new ItemStack[SIZE];
        for (int slot = 0; slot < SIZE; slot++) {
            contents[slot] = getItem(slot);
        }
        return contents;
    }

    @Override
    public void setContents(ItemStack[] items) {
        for (int slot = 0; slot < SIZE; slot++) {
            setItem(slot, items != null && slot < items.length ? items[slot] : null);
        }
    }

    @Override
    public boolean contains(Material material) {
        return first(material) >= 0;
    }

    @Override
    public int first(Material material) {
        if (material == null) {
            return -1;
        }
        for (int slot = 0; slot < SIZE; slot++) {
            ItemStack item = getItem(slot);
            if (item != null && item.getType() == material) {
                return slot;
            }
        }
        return -1;
    }

    @Override
    public void clear() {
        for (int slot = 0; slot < SIZE; slot++) {
            clear(slot);
        }
    }

    @Override
    public void clear(int slot) {
        Native.setInventorySlot(owner, slot, "");
    }

    @Override
    public ItemStack getItemInMainHand() {
        return getItem(getHeldItemSlot());
    }

    @Override
    public void setItemInMainHand(ItemStack item) {
        setItem(getHeldItemSlot(), item);
    }

    @Override
    public ItemStack getItemInHand() {
        return getItemInMainHand();
    }

    @Override
    public void setItemInHand(ItemStack item) {
        setItemInMainHand(item);
    }

    @Override
    public ItemStack getItemInOffHand() {
        return getItem(OFFHAND);
    }

    @Override
    public void setItemInOffHand(ItemStack item) {
        setItem(OFFHAND, item);
    }

    // Armor runs boots, leggings, chestplate, helmet from slot 36 upward,
    // which is the order the protocol uses and the reverse of how it reads.
    @Override
    public ItemStack getHelmet() {
        return getItem(ARMOR + 3);
    }

    @Override
    public ItemStack getChestplate() {
        return getItem(ARMOR + 2);
    }

    @Override
    public ItemStack getLeggings() {
        return getItem(ARMOR + 1);
    }

    @Override
    public ItemStack getBoots() {
        return getItem(ARMOR);
    }

    @Override
    public int getHeldItemSlot() {
        return Math.max(0, Native.heldSlot(owner));
    }

    /** Reads `minecraft:diamond_sword 3`. Anything else is an empty slot. */
    public static ItemStack decode(String text) {
        if (text == null || text.isEmpty()) {
            return null;
        }
        int space = text.lastIndexOf(' ');
        String name = space < 0 ? text : text.substring(0, space);
        int amount = 1;
        if (space >= 0) {
            try {
                amount = Integer.parseInt(text.substring(space + 1));
            } catch (NumberFormatException notANumber) {
                return null;
            }
        }
        Material material = Material.matchMaterial(name);
        if (material == null || material.isAir() || amount <= 0) {
            return null;
        }
        return new ItemStack(material, amount);
    }

    /** Writes what decode reads. An empty stack is an empty string. */
    public static String encode(ItemStack item) {
        if (item == null || item.getType().isAir() || item.getAmount() <= 0) {
            return "";
        }
        return "minecraft:" + item.getType().getKeyName() + " " + item.getAmount();
    }
}
