package foton;

import java.util.function.Supplier;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/** A stack read from a player's inventory that writes its edits back, as Paper's
 * {@code CraftItemStack.asCraftMirror} does: {@code hand.setAmount(hand.getAmount() - 1)}
 * is how plugins consume an item.
 *
 * <p>A mirror is a snapshot that remembers which slot it came from and what that
 * slot held then ({@code base}). Each edit is written back with
 * {@code Native.setInventorySlotIfUnchanged}, which replaces the slot only while it
 * still holds {@code base}. If the item has since moved, been replaced, been used
 * up or been merged into by another plugin or by the player, the write is refused
 * and the mirror becomes an ordinary copy: it can neither put a stale stack back
 * (a duplicate) nor overwrite whatever now sits in the slot. After a write the
 * mirror re-reads the slot, so it follows the server's own normalization and its
 * next edit is checked against what was just written.</p>
 *
 * <p>{@code clone()} and {@code new ItemStack(...)} give detached copies. Reads
 * are the snapshot's, not live; a changed slot shows up when the next edit is
 * refused.</p>
 */
final class FotonItemMirror extends ItemStack {
    private final String owner;
    private final int slot;
    /** What the slot held when last read or written; null once the mirror is detached. */
    private foton.item.ItemTransfer base;
    private int depth;

    private FotonItemMirror(ItemStack read, String owner, int slot, foton.item.ItemTransfer base) {
        super(read.getType(), read.getAmount());
        adopt(read);
        this.owner = owner;
        this.slot = slot;
        this.base = base;
    }

    /** The mirror of a slot's snapshot; null for an empty slot, which has nothing to edit. */
    static ItemStack of(String owner, int slot, foton.item.ItemTransfer transfer) {
        ItemStack read = FotonInventory.decodeTransfer(transfer);
        if (read == null || read.isEmpty()) return read;
        return new FotonItemMirror(read, owner, slot, transfer);
    }

    @Override public void setAmount(int amount) { edit(() -> { super.setAmount(amount); return null; }); }
    @Override public void setType(org.bukkit.Material type) { edit(() -> { super.setType(type); return null; }); }
    @Override public boolean setItemMeta(ItemMeta meta) { return edit(() -> super.setItemMeta(meta)); }
    @Override public void setDurability(short durability) { edit(() -> { super.setDurability(durability); return null; }); }
    @Override public <T> void setData(io.papermc.paper.datacomponent.DataComponentType.Valued<T> type, T value) {
        edit(() -> { super.setData(type, value); return null; });
    }
    @Override public void unsetData(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        edit(() -> { super.unsetData(type); return null; });
    }
    @Override public void resetData(io.papermc.paper.datacomponent.DataComponentType<?> type) {
        edit(() -> { super.resetData(type); return null; });
    }

    /** Runs one public edit, which may be built from others, and writes the result back once. */
    private <T> T edit(Supplier<T> change) {
        depth++;
        T result;
        try {
            result = change.get();
        } finally {
            depth--;
        }
        if (depth == 0) writeBack();
        return result;
    }

    private void writeBack() {
        if (base == null) return;
        if (!Native.setInventorySlotIfUnchanged(owner, slot, base.unchanged(), nativeMutation())) {
            base = null;
            return;
        }
        foton.item.ItemTransfer written = isEmpty() ? null : Native.inventorySlot(owner, slot);
        ItemStack current = FotonInventory.decodeTransfer(written);
        if (current == null || current.isEmpty()) {
            base = null;
            return;
        }
        adopt(current);
        base = written;
    }
}
