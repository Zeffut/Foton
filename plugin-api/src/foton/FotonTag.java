package foton;

import java.util.Collections;
import java.util.LinkedHashSet;
import java.util.Objects;
import java.util.Set;
import org.bukkit.Keyed;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.Tag;

/** A vanilla tag, answered by the server's registry rather than a copied list.
 *
 * <p>The data pack decides what is in {@code minecraft:logs}; asking the
 * registry Foton itself runs on means a plugin and the game can never
 * disagree about it.</p>
 */
public final class FotonTag<T extends Keyed> implements Tag<T> {
    private final String registry;
    private final NamespacedKey key;
    private final Class<T> type;

    public FotonTag(String registry, NamespacedKey key, Class<T> type) {
        this.registry = Objects.requireNonNull(registry, "registry");
        this.key = Objects.requireNonNull(key, "key");
        this.type = Objects.requireNonNull(type, "type");
    }

    /** A vanilla tag, as {@link Tag}'s constants name them. */
    public FotonTag(String registry, String path, Class<T> type) {
        this(registry, NamespacedKey.minecraft(path), type);
    }

    @Override
    public NamespacedKey getKey() {
        return key;
    }

    @Override
    public boolean isTagged(T value) {
        if (value == null || !type.isInstance(value)) return false;
        return Native.isTagged(registry, key.toString(), value.getKey().toString());
    }

    @Override
    public Set<T> getValues() {
        Set<T> values = new LinkedHashSet<>();
        String[] members = Native.tagValues(registry, key.toString());
        if (members != null) {
            for (String member : members) {
                T value = resolve(type, NamespacedKey.fromString(member));
                if (value != null) values.add(value);
            }
        }
        return Collections.unmodifiableSet(values);
    }

    /** The API value for a registry key, or null when the API has no value for it. */
    @SuppressWarnings("unchecked")
    static <T extends Keyed> T resolve(Class<T> type, NamespacedKey key) {
        if (key == null) return null;
        if (type == Material.class) return (T) Material.matchMaterial(key.toString());
        try {
            return org.bukkit.Bukkit.getRegistry(type).get(key);
        } catch (IllegalArgumentException unsupported) {
            return null;
        }
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonTag<?> tag && registry.equals(tag.registry) && key.equals(tag.key);
    }

    @Override
    public int hashCode() {
        return Objects.hash(registry, key);
    }

    @Override
    public String toString() {
        return "Tag{" + registry + ", " + key + "}";
    }
}
