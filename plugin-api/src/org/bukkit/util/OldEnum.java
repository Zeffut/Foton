package org.bukkit.util;

/** The enum-like surface Paper keeps on types that became registry interfaces.
 *
 * <p>Plugins written when these were enums still call {@code name()},
 * {@code ordinal()} and {@code compareTo()} on them. */
@Deprecated
public interface OldEnum<T extends OldEnum<T>> extends Comparable<T> {
    @Deprecated @Override int compareTo(T other);

    @Deprecated String name();

    @Deprecated int ordinal();
}
