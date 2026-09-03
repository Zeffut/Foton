package org.bukkit.entity;

/** Shared horse taming state. */
public interface AbstractHorse extends Animal {
    AnimalTamer getOwner();
    void setOwner(AnimalTamer owner);
    default double getJumpStrength() {
        org.bukkit.attribute.AttributeInstance value = getAttribute(org.bukkit.attribute.Attribute.GENERIC_JUMP_STRENGTH);
        return value == null ? 0.0 : value.getValue();
    }
    default void setJumpStrength(double strength) {
        org.bukkit.attribute.AttributeInstance value = getAttribute(org.bukkit.attribute.Attribute.GENERIC_JUMP_STRENGTH);
        if (value != null) value.setBaseValue(strength);
    }

    default int getDomestication() { return 0; }
    default void setDomestication(int value) { }
    default int getMaxDomestication() { return 100; }
    default void setMaxDomestication(int value) { }
}
