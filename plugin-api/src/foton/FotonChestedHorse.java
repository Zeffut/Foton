package foton;

import java.util.UUID;

/** Live Bukkit view for chested equines. */
public final class FotonChestedHorse extends FotonLivingEntity implements org.bukkit.entity.ChestedHorse {
    public FotonChestedHorse(UUID id) { super(id); }
    @Override public boolean isCarryingChest() { return Native.entityHasChest(getUniqueId().toString()); }
    @Override public void setCarryingChest(boolean carryingChest) { Native.entitySetChest(getUniqueId().toString(), carryingChest); }
}
