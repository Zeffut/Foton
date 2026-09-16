package foton;

import java.util.UUID;
import org.bukkit.inventory.ItemStack;

/** Live view of a large or small fireball's rendered item. */
public final class FotonSizedFireball extends FotonFireball
        implements org.bukkit.entity.SizedFireball {
    public FotonSizedFireball(UUID id) { super(id); }

    @Override public ItemStack getDisplayItem() {
        return FotonInventory.decode(Native.entityItemStack(getUniqueId().toString()));
    }

    @Override public void setDisplayItem(ItemStack item) {
        if (item == null) throw new IllegalArgumentException("item cannot be null");
        Native.setEntityItemStack(getUniqueId().toString(), FotonInventory.encode(item));
    }
}
