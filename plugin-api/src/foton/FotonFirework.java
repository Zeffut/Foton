package foton;

import java.util.UUID;
import org.bukkit.FireworkEffect;
import org.bukkit.inventory.meta.FireworkMeta;

/** Live Bukkit view of a Steel firework rocket. */
public final class FotonFirework extends FotonEntity implements org.bukkit.entity.Firework {
    public FotonFirework(UUID id) { super(id); }

    @Override public FireworkMeta getFireworkMeta() {
        String encoded = Native.fireworkMeta(getUniqueId().toString());
        FireworkMeta meta = new org.bukkit.inventory.meta.SimpleFireworkMeta();
        if (encoded == null || encoded.isEmpty()) return meta;
        String[] fields = encoded.split(";", 2);
        try { meta.setPower(Integer.parseInt(fields[0])); } catch (NumberFormatException ignored) { }
        if (fields.length < 2) return meta;
        for (String encodedEffect : fields[1].split(";")) {
            String[] effectFields = encodedEffect.split("\\|", -1);
            if (effectFields.length != 5) continue;
            FireworkEffect.Type type;
            try { type = FireworkEffect.Type.valueOf(effectFields[0]); }
            catch (IllegalArgumentException ignored) { continue; }
            FireworkEffect.Builder builder = FireworkEffect.builder().with(type)
                .trail(Boolean.parseBoolean(effectFields[3]))
                .flicker(Boolean.parseBoolean(effectFields[4]));
            appendDecodedColors(builder, effectFields[1], false);
            appendDecodedColors(builder, effectFields[2], true);
            meta.addEffect(builder.build());
        }
        return meta;
    }

    @Override public void setFireworkMeta(FireworkMeta meta) {
        if (meta == null) return;
        StringBuilder encoded = new StringBuilder();
        for (FireworkEffect effect : meta.getEffects()) {
            if (encoded.length() > 0) encoded.append(';');
            encoded.append(effect.getType().name()).append('|');
            appendColors(encoded, effect.getColors());
            encoded.append('|');
            appendColors(encoded, effect.getFadeColors());
            encoded.append('|').append(effect.hasTrail()).append('|').append(effect.hasFlicker());
        }
        Native.setFireworkMeta(getUniqueId().toString(), meta.getPower(), encoded.toString());
    }

    private static void appendColors(StringBuilder target, java.util.List<org.bukkit.Color> colors) {
        for (int i = 0; i < colors.size(); i++) {
            if (i > 0) target.append(',');
            target.append(colors.get(i).asRGB());
        }
    }

    private static void appendDecodedColors(FireworkEffect.Builder builder, String encoded, boolean fade) {
        if (encoded.isEmpty()) return;
        for (String value : encoded.split(",")) {
            try {
                org.bukkit.Color color = org.bukkit.Color.fromRGB(Integer.parseInt(value));
                if (fade) builder.withFade(color); else builder.withColor(color);
            } catch (NumberFormatException ignored) { }
        }
    }
}
