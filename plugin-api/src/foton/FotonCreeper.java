package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel creeper. */
public final class FotonCreeper extends FotonLivingEntity implements org.bukkit.entity.Creeper {
    public FotonCreeper(UUID id) { super(id); }
    @Override public boolean isPowered() { return Native.creeperPowered(getUniqueId().toString()); }
    @Override public void setPowered(boolean powered) { Native.setCreeperPowered(getUniqueId().toString(), powered); }
    @Override public org.bukkit.entity.Entity getIgniter() { return null; }
}
