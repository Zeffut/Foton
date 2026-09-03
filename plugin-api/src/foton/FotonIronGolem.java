package foton;

import java.util.UUID;

/** Live Bukkit view of an iron golem. */
public final class FotonIronGolem extends FotonLivingEntity implements org.bukkit.entity.IronGolem {
    public FotonIronGolem(UUID id) { super(id); }
    @Override public boolean isPlayerCreated() { return Native.ironGolemPlayerCreated(getUniqueId().toString()); }
    @Override public void setPlayerCreated(boolean value) { Native.setIronGolemPlayerCreated(getUniqueId().toString(), value); }
}
