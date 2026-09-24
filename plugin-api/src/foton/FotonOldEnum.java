package foton;

import java.util.Locale;
import org.bukkit.NamespacedKey;

/** A registry value that answers the enum questions Paper's {@code OldEnum} keeps.
 *
 * <p>The name is the key's path in upper case, which is what the enum
 * constant was called, and the ordinal is the registry index. Values are
 * unique per key (see {@link FotonKeyedRegistry}), so identity is equality.</p>
 */
public abstract class FotonOldEnum {
    private final NamespacedKey key;
    private final int ordinal;

    protected FotonOldEnum(String path, int ordinal) {
        this.key = NamespacedKey.minecraft(path);
        this.ordinal = ordinal;
    }

    public NamespacedKey getKey() {
        return key;
    }

    public String name() {
        return key.getKey().toUpperCase(Locale.ROOT);
    }

    public int ordinal() {
        return ordinal;
    }

    protected int compareOrdinal(FotonOldEnum other) {
        return Integer.compare(ordinal, other.ordinal);
    }

    @Override
    public String toString() {
        return name();
    }
}
