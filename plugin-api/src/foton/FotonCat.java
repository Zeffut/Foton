package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla cat. */
public final class FotonCat extends FotonTameableEntity implements org.bukkit.entity.Cat {
    public FotonCat(UUID id) { super(id); }
    @Override public Type getCatType() {
        String value = Native.catVariant(getUniqueId().toString());
        if (value == null) return Type.TABBY;
        try { return Type.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Type.TABBY; }
    }
    @Override public void setCatType(Type type) {
        if (type != null) Native.setCatVariant(getUniqueId().toString(), type.name());
    }
    @Override public boolean isSitting() { return Native.catSitting(getUniqueId().toString()); }
    @Override public void setSitting(boolean sitting) { Native.setCatSitting(getUniqueId().toString(), sitting); }
    @Override public org.bukkit.DyeColor getCollarColor() { return org.bukkit.DyeColor.values()[Native.catCollarColor(getUniqueId().toString())]; }
    @Override public void setCollarColor(org.bukkit.DyeColor color) { if (color != null) Native.setCatCollarColor(getUniqueId().toString(), color.ordinal()); }
}
