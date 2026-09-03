package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel goat. */
public final class FotonGoat extends FotonLivingEntity implements org.bukkit.entity.Goat {
    public FotonGoat(UUID id) { super(id); }
    @Override public boolean isScreaming() { return Native.goatScreaming(getUniqueId().toString()); }
    @Override public void setScreaming(boolean screaming) { Native.setGoatScreaming(getUniqueId().toString(), screaming); }
    @Override public boolean hasLeftHorn() { return Native.goatLeftHorn(getUniqueId().toString()); }
    @Override public void setLeftHorn(boolean present) { Native.setGoatLeftHorn(getUniqueId().toString(), present); }
    @Override public boolean hasRightHorn() { return Native.goatRightHorn(getUniqueId().toString()); }
    @Override public void setRightHorn(boolean present) { Native.setGoatRightHorn(getUniqueId().toString(), present); }
}
