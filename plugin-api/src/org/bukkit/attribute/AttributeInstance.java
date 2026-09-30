package org.bukkit.attribute;

import java.util.Collection;

/** A live view of one attribute on one entity. */
public interface AttributeInstance {
    Attribute getAttribute();

    double getBaseValue();

    void setBaseValue(double value);

    Collection<AttributeModifier> getModifiers();

    void removeModifier(net.kyori.adventure.key.Key key);

    void removeModifier(java.util.UUID uuid);

    /** Adds a modifier that is saved with the entity. */
    void addModifier(AttributeModifier modifier);

    /** Adds a modifier that lives until the entity is unloaded. */
    void addTransientModifier(AttributeModifier modifier);

    void removeModifier(AttributeModifier modifier);

    /** The base value with every modifier applied, clamped to the attribute's range. */
    double getValue();

    /** The attribute's registry default. */
    double getDefaultValue();
}
