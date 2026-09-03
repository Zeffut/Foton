package foton;

import java.util.UUID;

/** Live Bukkit view of a vanilla horse. */
public final class FotonHorse extends FotonLivingEntity implements org.bukkit.entity.Horse, org.bukkit.inventory.InventoryHolder {
    public FotonHorse(UUID id) { super(id); }
    @Override public int getDomestication() { return Native.horseTemper(getUniqueId().toString()); }
    @Override public void setDomestication(int value) { Native.setHorseTemper(getUniqueId().toString(), value); }
    @Override public int getMaxDomestication() { return Native.horseMaxTemper(getUniqueId().toString()); }
    @Override public org.bukkit.inventory.HorseInventory getInventory() { return new FotonHorseInventory(getUniqueId().toString()); }
    @Override public org.bukkit.entity.AnimalTamer getOwner() {
        String owner = Native.entityOwner(getUniqueId().toString());
        if (owner == null) return null;
        try { return new FotonAnimalTamer(UUID.fromString(owner), owner); }
        catch (IllegalArgumentException ignored) { return null; }
    }
    @Override public void setOwner(org.bukkit.entity.AnimalTamer owner) {
        Native.setEntityOwner(getUniqueId().toString(), owner == null ? null : owner.getUniqueId().toString());
    }
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
