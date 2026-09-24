package foton;

import java.util.Locale;
import java.util.UUID;
import org.bukkit.inventory.ItemStack;

/** Live Bukkit view of an item display. */
public final class FotonItemDisplay extends FotonDisplay implements org.bukkit.entity.ItemDisplay {
    public FotonItemDisplay(UUID id) { super(id); }

    @Override public ItemStack getItemStack() {
        ItemStack item = FotonInventory.decode(Native.itemDisplayItem(getUniqueId().toString()));
        return item == null ? new ItemStack(org.bukkit.Material.AIR) : item;
    }

    @Override public void setItemStack(ItemStack item) {
        Native.setItemDisplayItem(getUniqueId().toString(), FotonInventory.encode(item));
    }

    @Override public ItemDisplayTransform getItemDisplayTransform() {
        String name = Native.itemDisplayTransform(getUniqueId().toString());
        if (name == null) return ItemDisplayTransform.NONE;
        try { return ItemDisplayTransform.valueOf(name.toUpperCase(Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return ItemDisplayTransform.NONE; }
    }

    @Override public void setItemDisplayTransform(ItemDisplayTransform display) {
        if (display == null) throw new IllegalArgumentException("Display cannot be null");
        Native.setItemDisplayTransform(getUniqueId().toString(), display.name().toLowerCase(Locale.ROOT));
    }
}
