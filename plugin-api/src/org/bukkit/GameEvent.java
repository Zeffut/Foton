package org.bukkit;

/** Vanilla game event key. */
public final class GameEvent implements Keyed {
    public static final GameEvent CONTAINER_CLOSE = new GameEvent("container_close");
    public static final GameEvent CONTAINER_OPEN = new GameEvent("container_open");
    private final NamespacedKey key;
    private GameEvent(String name) { key = NamespacedKey.minecraft(name); }
    @Override public NamespacedKey getKey() { return key; }
}
