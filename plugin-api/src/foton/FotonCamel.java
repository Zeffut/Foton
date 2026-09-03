package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla camel. */
public final class FotonCamel extends FotonTameableEntity implements org.bukkit.entity.Camel {
    public FotonCamel(UUID id) { super(id); }
    @Override public org.bukkit.inventory.AbstractHorseInventory getInventory() { return new FotonHorseInventory(getUniqueId().toString()); }
}
