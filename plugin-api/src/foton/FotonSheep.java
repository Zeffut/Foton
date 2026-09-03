package foton;

import java.util.UUID;

/** Live Bukkit view of a sheep. */
public final class FotonSheep extends FotonLivingEntity implements org.bukkit.entity.Sheep {
    public FotonSheep(UUID id) { super(id); }
    @Override public org.bukkit.DyeColor getColor() {
        int value = Native.sheepColor(getUniqueId().toString());
        org.bukkit.DyeColor[] values = org.bukkit.DyeColor.values();
        return value >= 0 && value < values.length ? values[value] : org.bukkit.DyeColor.WHITE;
    }
    @Override public void setColor(org.bukkit.DyeColor color) { if (color != null) Native.setSheepColor(getUniqueId().toString(), color.ordinal()); }
    @Override public boolean isSheared() { return Native.sheepSheared(getUniqueId().toString()); }
    @Override public void setSheared(boolean sheared) { Native.setSheepSheared(getUniqueId().toString(), sheared); }
}
