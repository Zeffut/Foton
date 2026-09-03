package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla chicken. */
public final class FotonChicken extends FotonLivingEntity implements org.bukkit.entity.Chicken {
    public FotonChicken(UUID id) { super(id); }
    @Override public Variant getVariant() {
        String value = Native.chickenVariant(getUniqueId().toString());
        if (value == null) return Variant.TEMPERATE;
        try { return Variant.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.TEMPERATE; }
    }
    @Override public void setVariant(Variant variant) {
        if (variant != null) Native.setChickenVariant(getUniqueId().toString(), variant.name());
    }
}
