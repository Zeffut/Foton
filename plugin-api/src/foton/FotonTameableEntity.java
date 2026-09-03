package foton;

import java.util.UUID;

/** Living wrapper used only for vanilla tameable entity types. */
public class FotonTameableEntity extends FotonLivingEntity implements org.bukkit.entity.Tameable {
    public FotonTameableEntity(UUID id) { super(id); }
    @Override public boolean isTamed() { return Native.entityIsTamed(getUniqueId().toString()); }
    @Override public void setTamed(boolean tamed) { Native.setEntityTamed(getUniqueId().toString(), tamed); }
    @Override public org.bukkit.entity.AnimalTamer getOwner() {
        String owner = Native.entityOwner(getUniqueId().toString());
        if (owner == null) return null;
        try { return new FotonAnimalTamer(UUID.fromString(owner), owner); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public void setOwner(org.bukkit.entity.AnimalTamer owner) {
        Native.setEntityOwner(getUniqueId().toString(), owner == null ? null : owner.getUniqueId().toString());
    }
}
