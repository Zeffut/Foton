package foton;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.UUID;
import org.bukkit.entity.Arrow;
import org.bukkit.potion.PotionEffect;
import org.bukkit.potion.PotionEffectType;

/** Arrow handle backed by the arrow's server-side mob-effect list. */
public final class FotonArrow extends FotonProjectile implements Arrow {
    public FotonArrow(UUID id) { super(id); }

    @Override public org.bukkit.potion.PotionData getBasePotionData() {
        String value = Native.arrowPotion(getUniqueId().toString());
        if (value == null) return null;
        String name = value.substring(value.indexOf(':') + 1).toUpperCase(java.util.Locale.ROOT);
        boolean extended = name.startsWith("LONG_");
        boolean upgraded = name.startsWith("STRONG_");
        if (extended) name = name.substring(5);
        if (upgraded) name = name.substring(7);
        try { return new org.bukkit.potion.PotionData(org.bukkit.potion.PotionType.valueOf(name), extended, upgraded); }
        catch (IllegalArgumentException ignored) { return null; }
    }

    @Override public org.bukkit.Color getColor() {
        int raw = Native.arrowPotionColor(getUniqueId().toString());
        return raw < 0 ? null : org.bukkit.Color.fromRGB(raw & 0x00ffffff);
    }

    @Override public List<PotionEffect> getCustomEffects() {
        String[] encoded = Native.arrowCustomEffects(getUniqueId().toString());
        if (encoded == null) return List.of();
        ArrayList<PotionEffect> result = new ArrayList<>();
        for (String value : encoded) {
            String[] fields = value.split("\\|", -1);
            if (fields.length != 6) continue;
            try {
                PotionEffectType type = PotionEffectType.getByName(fields[0]);
                if (type != null) result.add(new PotionEffect(type,
                    Integer.parseInt(fields[1]), Integer.parseInt(fields[2]),
                    Boolean.parseBoolean(fields[3]), Boolean.parseBoolean(fields[4]),
                    Boolean.parseBoolean(fields[5])));
            } catch (NumberFormatException ignored) { }
        }
        return Collections.unmodifiableList(result);
    }
}
