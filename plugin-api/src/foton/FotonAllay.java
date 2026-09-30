package foton;

import java.util.UUID;

/** Live Bukkit view of an allay. */
public final class FotonAllay extends FotonLivingEntity implements org.bukkit.entity.Allay {
    public FotonAllay(UUID id) { super(id); }

    /** The one slot an allay carries. */
    @Override public org.bukkit.inventory.Inventory getInventory() { return new FotonCarriedInventory(this); }
}
