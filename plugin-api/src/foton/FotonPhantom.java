package foton;

import java.util.UUID;

/** Live Bukkit view of a phantom. */
public final class FotonPhantom extends FotonLivingEntity implements org.bukkit.entity.Phantom {
    public FotonPhantom(UUID id) { super(id); }
    @Override public int getSize() { return Native.phantomSize(getUniqueId().toString()); }
    @Override public void setSize(int size) { Native.setPhantomSize(getUniqueId().toString(), size); }
}
