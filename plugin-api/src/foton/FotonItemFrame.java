package foton;

import java.util.UUID;
import org.bukkit.entity.ItemFrame;
import org.bukkit.inventory.ItemStack;

/** Live item frame entity. */
public final class FotonItemFrame extends FotonEntity implements ItemFrame {
    FotonItemFrame(UUID id) { super(id); }
    @Override public ItemStack getItem() { return FotonInventory.decode(Native.entityItemStack(getUniqueId().toString())); }
}
