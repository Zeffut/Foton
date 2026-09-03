package org.bukkit.entity;

/** A fox with a vanilla biome variant. */
public interface Fox extends Animal {
    enum Type { RED, SNOW }
    default Type getFoxType() { return Type.RED; }
    default void setFoxType(Type type) { }
    boolean isSitting();
    void setSitting(boolean sitting);
}
