package org.bukkit;

import org.bukkit.plugin.Plugin;

/** A namespaced identifier.
 *
 * <p>An Adventure {@code Key}, as in Paper: Paper methods such as
 * {@code AttributeInstance.removeModifier(Key)} and {@code Player.getCooldown(Key)}
 * are handed a {@code NamespacedKey} by plugins and call it through the
 * {@code Key} interface. */
public final class NamespacedKey implements net.kyori.adventure.key.Key {
    private final String namespace;
    private final String key;

    public NamespacedKey(String namespace, String key) {
        this.namespace = namespace;
        this.key = key;
    }

    /** Creates a key in the plugin's namespace, as Bukkit plugins commonly do. */
    public NamespacedKey(Plugin plugin, String key) {
        if (plugin == null) {
            throw new IllegalArgumentException("plugin cannot be null");
        }
        this.namespace = plugin.getName().toLowerCase(java.util.Locale.ROOT);
        this.key = key;
    }

    public String getNamespace() { return namespace; }
    public String getKey() { return key; }

    /** Reads `namespace:key`, defaulting the namespace to minecraft.
     *
     * Bukkit answers null for a string that is not a valid key rather than
     * throwing, because plugins call this on whatever a config file said.
     */
    public static NamespacedKey fromString(String text) {
        if (text == null || text.isEmpty()) {
            return null;
        }
        int colon = text.indexOf(':');
        String namespace = colon < 0 ? "minecraft" : text.substring(0, colon);
        String key = colon < 0 ? text : text.substring(colon + 1);
        return namespace.isEmpty() || key.isEmpty() ? null : new NamespacedKey(namespace, key);
    }

    public static NamespacedKey minecraft(String key) {
        return new NamespacedKey("minecraft", key);
    }

    @Override public boolean equals(Object other) {
        return other instanceof NamespacedKey key
            && namespace.equals(key.namespace) && this.key.equals(key.key);
    }

    @Override public int hashCode() {
        return java.util.Objects.hash(namespace, key);
    }

    @Override public String toString() { return namespace + ":" + key; }
    @Override public String asString() { return toString(); }
    @Override public String namespace() { return namespace; }
    @Override public String value() { return key; }
}
