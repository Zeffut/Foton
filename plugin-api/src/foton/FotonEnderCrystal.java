package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel end crystal. */
public final class FotonEnderCrystal extends FotonEntity implements org.bukkit.entity.EnderCrystal {
    public FotonEnderCrystal(UUID id) { super(id); }
    @Override public boolean isShowingBottom() { return Native.endCrystalShowsBottom(getUniqueId().toString()); }
    @Override public void setShowingBottom(boolean showing) { Native.setEndCrystalShowsBottom(getUniqueId().toString(), showing); }
}
