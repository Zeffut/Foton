package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel slime. */
public final class FotonSlime extends FotonLivingEntity implements org.bukkit.entity.Slime {
    public FotonSlime(UUID id) { super(id); }
    @Override public int getSize() { return Native.slimeSize(getUniqueId().toString()); }
    @Override public void setSize(int size) {
        if (size > 0) Native.setSlimeSize(getUniqueId().toString(), size);
    }
}
