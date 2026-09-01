package foton;

import org.bukkit.NamespacedKey;
import org.bukkit.World;

/** A world, named. */
public final class FotonWorld implements World {
    private final String key;

    public FotonWorld(String key) { this.key = key; }

    @Override public String getName() {
        int colon = key.indexOf(':');
        return colon < 0 ? key : key.substring(colon + 1);
    }

    @Override public NamespacedKey getKey() {
        int colon = key.indexOf(':');
        return colon < 0
            ? new NamespacedKey("minecraft", key)
            : new NamespacedKey(key.substring(0, colon), key.substring(colon + 1));
    }
}
