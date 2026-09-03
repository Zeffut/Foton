package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla wolf. */
public final class FotonWolf extends FotonTameableEntity implements org.bukkit.entity.Wolf {
    @Override public boolean isAngry() { return Native.wolfAngry(getUniqueId().toString()); }
    @Override public void setAngry(boolean angry) { Native.setWolfAngry(getUniqueId().toString(), angry); }

    public FotonWolf(UUID id) { super(id); }
    @Override public Variant getVariant() {
        String value = Native.wolfVariant(getUniqueId().toString());
        if (value == null) return Variant.PALE;
        try { return Variant.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Variant.PALE; }
    }
    @Override public void setVariant(Variant variant) {
        if (variant != null) Native.setWolfVariant(getUniqueId().toString(), variant.name());
    }

    @Override public boolean isSitting() { return Native.wolfSitting(getUniqueId().toString()); }
    @Override public void setSitting(boolean sitting) { Native.setWolfSitting(getUniqueId().toString(), sitting); }
}
