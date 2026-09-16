package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel slime. */
public final class FotonSlime extends FotonLivingEntity implements org.bukkit.entity.Slime {
    public FotonSlime(UUID id) { super(id); }
    @Override public int getSize() { return Native.slimeSize(getUniqueId().toString()); }
    @Override public void setSize(int size) {
        Native.setSlimeSize(getUniqueId().toString(), size);
    }
    @Override public boolean canWander() {
        return Native.cubeMobCanWander(getUniqueId().toString());
    }
    @Override public void setWander(boolean canWander) {
        Native.setCubeMobWander(getUniqueId().toString(), canWander);
    }
}
