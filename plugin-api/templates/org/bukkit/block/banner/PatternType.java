package org.bukkit.block.banner;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;
import org.bukkit.util.OldEnum;

/** A banner pattern, one entry of the vanilla {@code banner_pattern} registry.
 *
 * <p>An interface, as in Paper: plugins call it with {@code invokeinterface}.
 * Its constants are generated from the data pack by
 * {@code dev/gen-registry-values.py}.</p>
 *
 * <p>It declares no default method, on purpose. A default method would make
 * creating a {@link foton.FotonPatternType} initialise this interface, whose
 * constants read the registry that is creating it.</p>
 */
public interface PatternType extends OldEnum<PatternType>, Keyed {
    // @@CONSTANTS@@

    @Override
    NamespacedKey getKey();

    /** Bukkit's pre-1.20.5 short code for the pattern.
     *
     * <p>Paper still answers with the legacy two-letter codes ({@code "bs"}).
     * That table is not vanilla data and nothing in this repository holds it,
     * so Foton answers with the namespaced key, which is also what
     * {@link #getByIdentifier(String)} accepts back.</p>
     */
    @Deprecated
    String getIdentifier();

    @Deprecated
    static PatternType getByIdentifier(String identifier) {
        if (identifier == null) return null;
        for (PatternType type : org.bukkit.Registry.BANNER_PATTERN) {
            if (identifier.equals(type.getIdentifier())) return type;
        }
        return null;
    }

    @Deprecated
    static PatternType valueOf(String name) {
        return foton.FotonRegistries.valueOf(org.bukkit.Registry.BANNER_PATTERN, name);
    }

    @Deprecated
    static PatternType[] values() {
        return org.bukkit.Registry.BANNER_PATTERN.stream().toArray(PatternType[]::new);
    }
}
