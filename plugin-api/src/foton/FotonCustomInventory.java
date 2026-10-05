package foton;

import java.util.HashMap;
import org.bukkit.Material;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryHolder;
import org.bukkit.inventory.ItemStack;

/** Mutable Bukkit inventory used before it is attached to an open menu. */
public final class FotonCustomInventory implements Inventory {
    private static final java.util.concurrent.ConcurrentHashMap<String, ViewerAttachment> OPEN =
        new java.util.concurrent.ConcurrentHashMap<>();
    private final InventoryHolder holder;
    private final ItemStack[] contents;
    private final String title;
    /** The title as the client is sent it: JSON text, colors and all. */
    private final String titleJson;
    private String viewer;
    private long nextAttachment;
    private long attachment;

    static final class ViewerAttachment {
        private final FotonCustomInventory inventory;
        private final long token;

        private ViewerAttachment(FotonCustomInventory inventory, long token) {
            this.inventory = inventory;
            this.token = token;
        }

        FotonCustomInventory inventory() { return inventory; }
        boolean matches(FotonCustomInventory expected, long expectedToken) {
            return inventory == expected && token == expectedToken;
        }
    }
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
        if (viewer != null && PluginHost.itemBridgeBound() && Native.openMenuTopSlotCount(viewer) == contents.length) return FotonInventory.decodeTransfer(Native.openMenuSlot(viewer, slot));
        return contents[slot] == null ? null : contents[slot].clone();
    }
    @Override public void setItem(int slot, ItemStack item) {
        if (slot < 0 || slot >= contents.length) return;
        ItemStack replacement = item == null ? null : item.clone();
        if (viewer != null && PluginHost.itemBridgeBound() && Native.openMenuTopSlotCount(viewer) == contents.length)
            Native.setOpenMenuItems(viewer, new int[]{slot}, new foton.item.ItemMutation[]{FotonInventory.mutation(replacement)}, false);
        contents[slot] = replacement;
    }
    @Override public HashMap<Integer, ItemStack> addItem(ItemStack... items) {
        HashMap<Integer, ItemStack> left = new HashMap<>(); if (items == null) return left;
        ItemStack[] staged = getContents();
        for (int index = 0; index < items.length; index++) { ItemStack in = items[index] == null ? null : items[index].clone(); if (in == null) continue;
            for (int slot = 0; slot < contents.length && in.getAmount() > 0; slot++) { ItemStack current = staged[slot]; if (current != null && current.isSimilar(in)) { int moved = Math.min(in.getAmount(), Math.max(0, current.getMaxStackSize() - current.getAmount())); if (moved > 0) { current.setAmount(current.getAmount() + moved); in.setAmount(in.getAmount() - moved); staged[slot] = current; } } }
            for (int slot = 0; slot < contents.length && in.getAmount() > 0; slot++) if (staged[slot] == null || staged[slot].getType().isAir()) { int moved = Math.min(in.getAmount(), in.getMaxStackSize()); ItemStack placed = in.clone(); placed.setAmount(moved); staged[slot] = placed; in.setAmount(in.getAmount() - moved); }
            if (in.getAmount() > 0) left.put(index, in);
        } setContents(staged); return left;
    }
    @Override public ItemStack[] getContents() { ItemStack[] result = new ItemStack[contents.length]; for (int i = 0; i < result.length; i++) result[i] = getItem(i); return result; }
    @Override public void setContents(ItemStack[] items) {
        if (items != null && items.length > contents.length) throw new IllegalArgumentException("inventory contents exceed size");
        ItemStack[] replacement = new ItemStack[contents.length];
        for (int i = 0; i < replacement.length; i++) replacement[i] = items != null && i < items.length && items[i] != null ? items[i].clone() : null;
        if (viewer != null && PluginHost.itemBridgeBound() && Native.openMenuTopSlotCount(viewer) == contents.length) FotonMenuInventory.setNativeContents(viewer, contents.length, replacement);
        System.arraycopy(replacement, 0, contents, 0, contents.length);
    }
    @Override public boolean contains(Material material) { return first(material) >= 0; }
    @Override public int first(Material material) { if (material == null) return -1; for (int i = 0; i < contents.length; i++) if (contents[i] != null && contents[i].getType() == material) return i; return -1; }
    @Override public void clear() { setContents(null); }
    @Override public void clear(int slot) { if (slot >= 0 && slot < contents.length) setItem(slot, null); }
    public String getTitle() { return title; }
    String titleJson() { return titleJson; }
    synchronized void attachViewer(String uuid) {
        if (uuid == null) throw new IllegalArgumentException("viewer cannot be null");
        removeCurrentAttachment();
        nextAttachment++;
        if (nextAttachment == 0) nextAttachment++;
        viewer = uuid;
        attachment = nextAttachment;
        OPEN.put(uuid, new ViewerAttachment(this, attachment));
    }
    synchronized void detachViewer() {
        removeCurrentAttachment();
        viewer = null;
        attachment = 0;
    }
    private void removeCurrentAttachment() {
        String attachedViewer = viewer;
        if (attachedViewer != null) {
            ViewerAttachment current = OPEN.get(attachedViewer);
            if (current != null && current.matches(this, attachment)) {
                OPEN.remove(attachedViewer, current);
            }
        }
    }
    synchronized void detachViewer(ViewerAttachment expected) {
        if (expected == null || !expected.matches(this, attachment)) return;
        // The closing native menu is still readable for the close callback.
        // Save its actual contents before retiring this exact attachment.
        ItemStack[] last = getContents();
        System.arraycopy(last, 0, contents, 0, contents.length);
        String attachedViewer = viewer;
        if (attachedViewer != null) OPEN.remove(attachedViewer, expected);
        viewer = null;
        attachment = 0;
    }
    static ViewerAttachment openAttachmentForViewer(String uuid) {
        if (uuid == null) return null;
        ViewerAttachment current = OPEN.get(uuid);
        if (current != null && current.inventory() == null) {
            OPEN.remove(uuid, current);
            return null;
        }
        return current;
    }
    static FotonCustomInventory openForViewer(String uuid) {
        ViewerAttachment current = openAttachmentForViewer(uuid);
        return current == null ? null : current.inventory();
    }
    String encodeContents() { StringBuilder result = new StringBuilder(); for (int i = 0; i < contents.length; i++) { if (i > 0) result.append('\u001e'); result.append(FotonInventory.encode(contents[i])); } return result.toString(); }
}
