package org.bukkit.inventory.meta.trim;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;
import org.bukkit.Translatable;

/** The pattern of an armor trim, one entry of the vanilla {@code trim_pattern} registry.
 *
 * <p>An interface, as in Paper. Its constants are generated from the data pack
 * by {@code dev/gen-registry-values.py}; it declares no default method, for
 * the reason {@link org.bukkit.block.banner.PatternType} gives.</p>
 */
public interface TrimPattern extends Keyed, Translatable {
    // @@CONSTANTS@@

    /** The name the client shows. */
    net.kyori.adventure.text.Component description();

    @Override
    @Deprecated
    String getTranslationKey();

    @Override
    @Deprecated
    NamespacedKey getKey();
}
