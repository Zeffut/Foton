package org.bukkit.block;

import org.bukkit.Keyed;
import org.bukkit.NamespacedKey;
import org.bukkit.util.OldEnum;

/** A biome, one entry of the vanilla {@code worldgen/biome} registry.
 *
 * <p>An interface, as in Paper: plugins call {@link #getKey()} with
 * {@code invokeinterface}. Its constants are generated from the data pack by
 * {@code dev/gen-registry-values.py}.</p>
 *
 * <p>It declares no default method, on purpose: see
 * {@link org.bukkit.block.banner.PatternType}. Paper's
 * {@code translationKey()} default is implemented by the value class.</p>
 */
public interface Biome extends OldEnum<Biome>, Keyed, net.kyori.adventure.translation.Translatable {
    // @@CONSTANTS@@

    @Override
    NamespacedKey getKey();

    @Deprecated
    static Biome valueOf(String name) {
        return foton.FotonRegistries.valueOf(org.bukkit.Registry.BIOME, name);
    }

    @Deprecated
    static Biome[] values() {
        return org.bukkit.Registry.BIOME.stream().toArray(Biome[]::new);
    }
}
