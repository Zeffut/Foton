package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel axolotl. */
public final class FotonAxolotl extends FotonLivingEntity implements org.bukkit.entity.Axolotl {
    public FotonAxolotl(UUID id) { super(id); }
    @Override public Variant getVariant() {
        try { return Variant.valueOf(Native.axolotlVariant(getUniqueId().toString()).toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.LUCY; }
    }
    @Override public void setVariant(Variant variant) {
        if (variant != null) Native.setAxolotlVariant(getUniqueId().toString(), variant.name());
    }
}
