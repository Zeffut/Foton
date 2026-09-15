package org.bukkit.util;

/** Minimal Paper 26.2 OldEnum ABI fixture. */
public interface OldEnum<T extends OldEnum<T>> extends Comparable<T> {
    int compareTo(T other);
    String name();
    int ordinal();
}
