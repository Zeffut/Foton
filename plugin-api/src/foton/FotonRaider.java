package foton;

import java.util.UUID;

/** Live Bukkit view of a patrol/raid mob. */
public final class FotonRaider extends FotonLivingEntity implements org.bukkit.entity.Raider {
    public FotonRaider(UUID id) { super(id); }
    @Override public boolean isPatrolLeader() { return Native.raiderPatrolLeader(getUniqueId().toString()); }
    @Override public void setPatrolLeader(boolean leader) { Native.setRaiderPatrolLeader(getUniqueId().toString(), leader); }
}
