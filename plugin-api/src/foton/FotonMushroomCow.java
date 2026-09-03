package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel mooshroom. */
public final class FotonMushroomCow extends FotonLivingEntity implements org.bukkit.entity.MushroomCow {
    public FotonMushroomCow(UUID id) { super(id); }
    @Override public Variant getVariant() {
        return "brown".equalsIgnoreCase(Native.mushroomCowVariant(getUniqueId().toString())) ? Variant.BROWN : Variant.RED;
    }
    @Override public void setVariant(Variant variant) {
        if (variant != null) Native.setMushroomCowVariant(getUniqueId().toString(), variant.name());
    }
}
