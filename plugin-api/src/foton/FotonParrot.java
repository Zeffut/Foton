package foton;

import java.util.UUID;

/** Live Bukkit view of a Steel parrot. */
public final class FotonParrot extends FotonTameableEntity implements org.bukkit.entity.Parrot {
    public FotonParrot(UUID id) { super(id); }
    @Override public Variant getVariant() {
        try {
            return switch (Native.parrotVariant(getUniqueId().toString()).toUpperCase(java.util.Locale.ROOT)) {
                case "RED_BLUE" -> Variant.RED; case "YELLOW_BLUE" -> Variant.CYAN;
                case "BLUE" -> Variant.BLUE; case "GREEN" -> Variant.GREEN; default -> Variant.GRAY;
            };
        } catch (RuntimeException ignored) { return Variant.RED; }
    }
    @Override public void setVariant(Variant variant) {
        if (variant == null) return;
        String value = switch (variant) { case RED -> "RED_BLUE"; case CYAN -> "YELLOW_BLUE"; default -> variant.name(); };
        Native.setParrotVariant(getUniqueId().toString(), value);
    }
}
