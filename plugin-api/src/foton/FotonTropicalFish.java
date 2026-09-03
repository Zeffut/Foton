package foton;

import java.util.UUID;
import org.bukkit.DyeColor;

/** Live Bukkit view of a Steel tropical fish. */
public final class FotonTropicalFish extends FotonLivingEntity implements org.bukkit.entity.TropicalFish {
    public FotonTropicalFish(UUID id) { super(id); }
    @Override public Pattern getPattern() {
        String value = Native.tropicalFishPattern(getUniqueId().toString());
        if (value == null) return Pattern.KOB;
        try { return Pattern.valueOf(value.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException ignored) { return Pattern.KOB; }
    }
    @Override public void setPattern(Pattern pattern) {
        if (pattern != null) Native.setTropicalFishPattern(getUniqueId().toString(), pattern.name());
    }
    @Override public DyeColor getPatternColor() {
        return DyeColor.getByWoolData((byte) Native.tropicalFishPatternColor(getUniqueId().toString()));
    }
    @Override public void setPatternColor(DyeColor color) {
        if (color != null) Native.setTropicalFishPatternColor(getUniqueId().toString(), color.getWoolData());
    }
    @Override public DyeColor getBodyColor() {
        return DyeColor.getByWoolData((byte) Native.tropicalFishBodyColor(getUniqueId().toString()));
    }
    @Override public void setBodyColor(DyeColor color) {
        if (color != null) Native.setTropicalFishBodyColor(getUniqueId().toString(), color.getWoolData());
    }
}
