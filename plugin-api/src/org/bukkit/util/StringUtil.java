package org.bukkit.util;

import java.util.Collection;

/** String helpers used by Bukkit tab completion. */
public final class StringUtil {
    private StringUtil() {}

    public static boolean startsWithIgnoreCase(String string, String prefix) {
        return string != null && prefix != null && string.regionMatches(true, 0, prefix, 0, prefix.length());
    }

    public static <T extends String> Collection<T> copyPartialMatches(
            String token, Iterable<T> originals, Collection<T> collection) {
        if (token == null || originals == null || collection == null) {
            throw new IllegalArgumentException("arguments must not be null");
        }
        String prefix = token.toLowerCase(java.util.Locale.ROOT);
        for (T original : originals) {
            if (original != null && original.toLowerCase(java.util.Locale.ROOT).startsWith(prefix)) {
                collection.add(original);
            }
        }
        return collection;
    }
}
