package foton;

import java.util.UUID;

/** Live Bukkit view of an armor stand. */
public final class FotonArmorStand extends FotonLivingEntity implements org.bukkit.entity.ArmorStand {
    public FotonArmorStand(UUID id) { super(id); }
    @Override public void setArms(boolean arms) { Native.armorStandSetArms(getUniqueId().toString(), arms); }
}
