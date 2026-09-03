package foton;

import java.util.UUID;

/** Live-backed nautilus entity. */
public final class FotonNautilus extends FotonTameableEntity implements org.bukkit.entity.Nautilus {
    public FotonNautilus(UUID id) { super(id); }
    @Override public org.bukkit.inventory.ArmoredSaddledMountInventory getInventory() { return new FotonHorseInventory(getUniqueId().toString()); }
}
