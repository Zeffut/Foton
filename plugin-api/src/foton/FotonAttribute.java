package foton;

import java.util.Locale;
import org.bukkit.NamespacedKey;
import org.bukkit.attribute.Attribute;

/** One entry of the vanilla attribute registry.
 *
 * <p>Built only by the generated {@link Attribute} constants, so there is one
 * object per attribute and identity comparison is what Paper's is. */
public final class FotonAttribute implements Attribute {
    private final NamespacedKey key;
    private final int ordinal;

    public FotonAttribute(String path, int ordinal) {
        this.key = NamespacedKey.minecraft(path);
        this.ordinal = ordinal;
    }

    @Override public NamespacedKey getKey() { return key; }

    /** Paper's registry-backed types answer {@code name()} with the upper-cased path. */
    @Override public String name() { return key.getKey().toUpperCase(Locale.ROOT); }

    @Override public int ordinal() { return ordinal; }

    @Override public int compareTo(Attribute other) { return Integer.compare(ordinal, other.ordinal()); }

    @Override public String translationKey() { return "attribute.name." + key.getKey(); }

    @Override public String toString() { return name(); }
}
