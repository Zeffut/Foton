package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Mutable Bukkit inventory used before it is attached to an open menu. */
public final class FotonCustomInventory implements Inventory {
    /** The plugin inventory each player has open. Held strongly, as the open
     * container holds it on Paper: a plugin commonly keeps no reference to
     * the inventory it opened. Released on close, and replaced by the next
     * inventory the same player opens, so it never holds more than one each. */
    private static final java.util.concurrent.ConcurrentHashMap<String, FotonCustomInventory> OPEN = new java.util.concurrent.ConcurrentHashMap<>();
    private final InventoryHolder holder;
    private final ItemStack[] contents;
    private final String title;
    /** The title as the client is sent it: JSON text, colours and all. */
    private final String titleJson;
    private String viewer;
    /** A title given as a string, which Bukkit reads with section-sign colours. */
    public FotonCustomInventory(InventoryHolder holder, int size, String title) {
        this(holder, size, net.kyori.adventure.text.serializer.legacy.LegacyComponentSerializer.legacySection()
            .deserialize(title == null ? "" : title));
    }
    public FotonCustomInventory(InventoryHolder holder, int size, net.kyori.adventure.text.Component title) {
        if (size < 1 || size > 54 || size % 9 != 0) throw new IllegalArgumentException("Inventory size must be a multiple of 9 between 1 and 54");
        net.kyori.adventure.text.Component name = title == null ? net.kyori.adventure.text.Component.empty() : title;
        this.holder = holder; this.contents = new ItemStack[size];
        this.title = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(name);
        this.titleJson = FotonText.json(name);
    }
    @Override public InventoryHolder getHolder() { return holder; }
    @Override public int getSize() { return contents.length; }
    @Override public InventoryType getType() { return InventoryType.CHEST; }
    @Override public ItemStack getItem(int slot) {
        if (slot < 0 || slot >= contents.length) return null;
        if (viewer != null && Native.openMenuTopSlotCount(viewer) == contents.length) return FotonInventory.decode(Native.openMenuSlot(viewer, slot));
        return contents[slot] == null ? null : contents[slot].clone();
    }
    @Override public void setItem(int slot, ItemStack item) {
        if (slot < 0 || slot >= contents.length) return;
        contents[slot] = item == null ? null : item.clone();
        if (viewer != null && Native.openMenuTopSlotCount(viewer) == contents.length) Native.setOpenMenuSlot(viewer, slot, FotonInventory.encode(item));
    }
    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>(); if (items == null) return left;
        for (int index = 0; index < items.length; index++) { ItemStack in = items[index] == null ? null : items[index].clone(); if (in == null) continue;
            for (int slot = 0; slot < contents.length && in.getAmount() > 0; slot++) { ItemStack current = getItem(slot); if (current != null && current.isSimilar(in)) { int moved = Math.min(in.getAmount(), Math.max(0, current.getMaxStackSize() - current.getAmount())); if (moved > 0) { current.setAmount(current.getAmount() + moved); in.setAmount(in.getAmount() - moved); setItem(slot, current); } } }
            for (int slot = 0; slot < contents.length && in.getAmount() > 0; slot++) if (getItem(slot) == null || getItem(slot).getType().isAir()) { int moved = Math.min(in.getAmount(), in.getMaxStackSize()); ItemStack placed = in.clone(); placed.setAmount(moved); setItem(slot, placed); in.setAmount(in.getAmount() - moved); }
            if (in.getAmount() > 0) left.put(index, in);
        } return left;
    }
    @Override public ItemStack[] getContents() { ItemStack[] result = new ItemStack[contents.length]; for (int i = 0; i < result.length; i++) result[i] = getItem(i); return result; }
    @Override public void setContents(ItemStack[] items) { for (int i = 0; i < contents.length; i++) setItem(i, items != null && i < items.length ? items[i] : null); }
    @Override public boolean contains(Material material) { return first(material) >= 0; }
    @Override public int first(Material material) { if (material == null) return -1; for (int i = 0; i < contents.length; i++) if (contents[i] != null && contents[i].getType() == material) return i; return -1; }
    @Override public void clear() { for (int i = 0; i < contents.length; i++) clear(i); }
    @Override public void clear(int slot) { if (slot >= 0 && slot < contents.length) setItem(slot, null); }
    public String getTitle() { return title; }
    String titleJson() { return titleJson; }
    /** The plugin inventory `uuid` has open, while its menu is still the one
     * opened for it; null otherwise. A view's top inventory is this object, so
     * a plugin finds its own holder on it, as it does on Paper. */
    static FotonCustomInventory viewedBy(String uuid) {
        FotonCustomInventory inventory = uuid == null ? null : OPEN.get(uuid);
        if (inventory == null || !uuid.equals(inventory.viewer)) return null;
        return Native.openMenuTopSlotCount(uuid) == inventory.contents.length ? inventory : null;
    }
    void attachViewer(String uuid) { viewer = uuid; OPEN.put(uuid, this); }
    void detachViewer() { if (viewer != null) OPEN.remove(viewer); viewer = null; }
    /** Lets go of `uuid` if it is still this inventory's viewer, keeping
     * contents as they were last seen; another inventory opened since keeps it. */
    void detachViewer(String uuid) {
        if (uuid == null || !uuid.equals(viewer)) return;
        for (int slot = 0; slot < contents.length; slot++) {
            ItemStack seen = getItem(slot);
            contents[slot] = seen == null ? null : seen.clone();
        }
        OPEN.remove(uuid, this);
        viewer = null;
    }
    String encodeContents() { StringBuilder result = new StringBuilder(); for (int i = 0; i < contents.length; i++) { if (i > 0) result.append('\u001e'); result.append(FotonInventory.encode(contents[i])); } return result.toString(); }
}
