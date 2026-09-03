package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel frog. */
public final class FotonFrog extends FotonLivingEntity implements org.bukkit.entity.Frog {
    public FotonFrog(UUID id) { super(id); }
    @Override public Variant getVariant() {
        String value = Native.frogVariant(getUniqueId().toString());
        if (value == null) return Variant.TEMPERATE;
        try { return Variant.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.TEMPERATE; }
    }
    @Override public void setVariant(Variant variant) { if (variant != null) Native.setFrogVariant(getUniqueId().toString(), variant.name()); }
}
