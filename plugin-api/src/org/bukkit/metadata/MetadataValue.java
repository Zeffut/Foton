package org.bukkit.metadata;

import org.bukkit.plugin.Plugin;

/** A value attached to a Bukkit object by a plugin. */
public interface MetadataValue {
    Object value();
    default byte asByte() { return ((Number) value()).byteValue(); }
    default short asShort() { return ((Number) value()).shortValue(); }
    default int asInt() { return ((Number) value()).intValue(); }
    default long asLong() { return ((Number) value()).longValue(); }
    default float asFloat() { return ((Number) value()).floatValue(); }
    default double asDouble() { return ((Number) value()).doubleValue(); }
    default boolean asBoolean() { return Boolean.parseBoolean(String.valueOf(value())); }
    default String asString() { return String.valueOf(value()); }
    Plugin getOwningPlugin();
    default void invalidate() { }
}
