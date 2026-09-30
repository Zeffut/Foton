package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla horse. */
public final class FotonHorse extends FotonAbstractHorse implements org.bukkit.entity.Horse {
    public FotonHorse(UUID id) { super(id); }
    @Override public org.bukkit.inventory.HorseInventory getInventory() { return new FotonHorseInventory(getUniqueId().toString()); }
    @Override public Color getColor() {
        String value = Native.horseVariant(getUniqueId().toString());
        if (value == null) return Color.WHITE;
        try { return Color.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Color.WHITE; }
    }
    @Override public void setColor(Color color) {
        if (color != null) Native.setHorseVariant(getUniqueId().toString(), color.name());
    }
    @Override public Style getStyle() {
        String value = Native.horseMarkings(getUniqueId().toString());
        if (value == null) return Style.NONE;
        try {
            if ("white_dots".equalsIgnoreCase(value)) return Style.WHITE_DOTS;
            return Style.valueOf(value.toUpperCase(java.util.Locale.ROOT));
        } catch (IllegalArgumentException ignored) { return Style.NONE; }
    }
    @Override public void setStyle(Style style) {
        if (style != null) Native.setHorseMarkings(getUniqueId().toString(), style.name());
    }
}
