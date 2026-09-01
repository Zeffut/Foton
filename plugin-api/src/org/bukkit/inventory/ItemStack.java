package org.bukkit.inventory;

import org.bukkit.Material;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.inventory.meta.SimpleItemMeta;

/** A stack of one material.
 *
 * The most-constructed type in the corpus: thirty-two of the fifty-nine
 * plugins surveyed call `new ItemStack`.
 *
 * Mutable, like Bukkit's, and a plugin that hands one to the server is handing
 * over a description rather than a live reference into a chest -- reading an
 * inventory gives copies, and writing one takes them.
 */
public class ItemStack implements Cloneable {
    private Material type;
    private int amount;
    private ItemMeta meta;

    public ItemStack(Material type) {
        this(type, 1);
    }

    public ItemStack(Material type, int amount) {
        this.type = type == null ? Material.AIR : type;
        this.amount = amount;
    }

    public Material getType() {
        return type;
    }

    /** Changing the type to air empties the stack, which is what Bukkit does
     * and what a plugin clearing a slot relies on. */
    public void setType(Material type) {
        this.type = type == null ? Material.AIR : type;
        if (this.type.isAir()) {
            this.amount = 0;
            this.meta = null;
        }
    }

    public int getAmount() {
        return amount;
    }

    public void setAmount(int amount) {
        this.amount = amount;
    }

    public int getMaxStackSize() {
        return type.getMaxStackSize();
    }

    public boolean hasItemMeta() {
        return meta != null;
    }

    /** A copy of the meta, or a fresh empty one.
     *
     * A copy because Bukkit's is: a plugin mutates what it gets and then calls
     * setItemMeta, and a plugin that forgets the second call sees no change.
     * That is a trap, it is Bukkit's trap, and behaving differently here would
     * make plugins written against it silently wrong instead.
     */
    public ItemMeta getItemMeta() {
        return meta == null ? new SimpleItemMeta() : meta.clone();
    }

    public boolean setItemMeta(ItemMeta value) {
        this.meta = value == null ? null : value.clone();
        return true;
    }

    /** Whether two stacks are the same item, ignoring how many. */
    public boolean isSimilar(ItemStack other) {
        return other != null
            && type == other.type
            && java.util.Objects.equals(meta, other.meta);
    }

    @Override
    public ItemStack clone() {
        ItemStack copy = new ItemStack(type, amount);
        copy.meta = meta == null ? null : meta.clone();
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof ItemStack stack
            && amount == stack.amount
            && isSimilar(stack);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(type, amount, meta);
    }

    @Override
    public String toString() {
        return "ItemStack{" + type + " x " + amount + "}";
    }
}
