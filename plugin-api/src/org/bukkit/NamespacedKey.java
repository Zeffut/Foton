package org.bukkit;

public final class NamespacedKey {
    private final String namespace;
    private final String key;

    public NamespacedKey(String namespace, String key) {
        this.namespace = namespace;
        this.key = key;
    }

    public String getNamespace() { return namespace; }
    public String getKey() { return key; }

    @Override public String toString() { return namespace + ":" + key; }
}
