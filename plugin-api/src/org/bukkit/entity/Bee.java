package org.bukkit.entity;

/** Bee entity. */
public interface Bee extends Animal {
    default int getAnger() { return 0; }
    default void setAnger(int anger) { }
    default boolean hasNectar() { return false; }
    default void setHasNectar(boolean hasNectar) { }
    default boolean hasStung() { return false; }
    default void setHasStung(boolean hasStung) { }
}
