package org.bukkit.util;

/** Compatibility contract used by registry-backed replacements for former enums. */
public interface OldEnum<T extends OldEnum<T>> extends Comparable<T> {
    @Override int compareTo(T other);
    String name();
    int ordinal();
}
