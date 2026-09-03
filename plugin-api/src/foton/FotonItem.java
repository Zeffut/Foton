package foton;

import java.util.UUID;
import org.bukkit.entity.Item;

/** Item entity handle backed by a UUID. */
public final class FotonItem extends FotonEntity implements Item {
    public FotonItem(UUID id) { super(id); }
    @Override public org.bukkit.inventory.ItemStack getItemStack() {
        return FotonInventory.decode(Native.entityItemStack(getUniqueId().toString()));
    }
    @Override public void setUnlimitedLifetime(boolean unlimited) { Native.setItemUnlimitedLifetime(getUniqueId().toString(), unlimited); }
    @Override public void setItemStack(org.bukkit.inventory.ItemStack item) {
        Native.setEntityItemStack(getUniqueId().toString(), FotonInventory.encode(item));
    }
}
