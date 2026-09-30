package foton;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BiFunction;
import java.util.stream.Stream;
import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;

/** A vanilla registry whose entries are generated into {@link FotonRegistryData}.
 *
 * <p>Values are created on first use and then kept, so each key has exactly
 * one value and a plugin comparing with {@code ==} gets Paper's answer.</p>
 *
 * <p>No lock is held while a value is created. Creating one can initialise the
 * API type it implements, whose constants read this same registry from the
 * same thread (see {@link org.bukkit.MusicInstrument}); a lock held across that
 * would either deadlock or hand the inner read a half-built registry. Instead
 * the first value stored for a key wins and any other copy is dropped.</p>
 */
public final class FotonKeyedRegistry<T extends Keyed> implements Registry<T> {
    private final String[] keys;
    /** Builds the value for (path, index in registry order). */
    private final BiFunction<String, Integer, T> factory;
    private final Map<String, T> values = new HashMap<>();

    public FotonKeyedRegistry(String[] keys, BiFunction<String, Integer, T> factory) {
        this.keys = keys.clone();
        this.factory = factory;
    }

    public int size() {
        return keys.length;
    }

    @Override
    public T get(NamespacedKey key) {
        if (key == null || !NamespacedKey.MINECRAFT.equals(key.getNamespace())) return null;
        return lookup(key.getKey());
    }

    @Override
    public Stream<T> stream() {
        List<T> all = new ArrayList<>(keys.length);
        for (String key : keys) all.add(lookup(key));
        return all.stream();
    }

    private T lookup(String path) {
        int index = indexOf(path);
        if (index < 0) return null;
        synchronized (values) {
            T existing = values.get(path);
            if (existing != null) return existing;
        }
        T created = factory.apply(path, index);
        synchronized (values) {
            T existing = values.putIfAbsent(path, created);
            return existing != null ? existing : created;
        }
    }

    private int indexOf(String path) {
        for (int index = 0; index < keys.length; index++) {
            if (keys[index].equals(path)) return index;
        }
        return -1;
    }
}
