package org.bukkit.util;

/** What a value that used to be an enum keeps of one.
 *
 * <p>Paper's bridge for registry values that were enums before 1.21: plugins
 * written then still call {@code name()}, {@code ordinal()} and
 * {@code compareTo}, and those calls resolve here.</p>
 */
@Deprecated
public interface OldEnum<T extends OldEnum<T>> extends Comparable<T> {
    @Deprecated
    @Override
    int compareTo(T other);

    @Deprecated
    String name();

    @Deprecated
    int ordinal();
}
