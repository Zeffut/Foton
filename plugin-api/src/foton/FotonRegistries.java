package foton;

import java.util.Locale;
import net.kyori.adventure.text.Component;
import net.kyori.adventure.text.format.TextColor;
import org.bukkit.Keyed;
import org.bukkit.MusicInstrument;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.block.Biome;
import org.bukkit.block.banner.PatternType;
import org.bukkit.inventory.meta.trim.TrimMaterial;
import org.bukkit.inventory.meta.trim.TrimPattern;

/** Builds the data-driven registries {@link Registry} holds, from generated data.
 *
 * <p>Stateless on purpose. The registries live in {@code Registry}'s own
 * fields, which every constant of {@code PatternType}, {@code Biome} and the
 * rest reads: a second copy here would be a class that the constants and
 * {@code Registry} each need initialised before the other, and whichever a
 * plugin touched first would see the other's fields still null.</p>
 */
public final class FotonRegistries {
    private FotonRegistries() { }

    public static Registry<PatternType> bannerPatterns() {
        return new FotonKeyedRegistry<>(FotonRegistryData.BANNER_PATTERN_KEYS, FotonPatternType::new);
    }

    public static Registry<Biome> biomes() {
        return new FotonKeyedRegistry<>(FotonRegistryData.BIOME_KEYS, FotonBiome::new);
    }

    public static Registry<TrimMaterial> trimMaterials() {
        return new FotonKeyedRegistry<>(FotonRegistryData.TRIM_MATERIAL_KEYS, FotonTrimMaterial::new);
    }

    public static Registry<TrimPattern> trimPatterns() {
        return new FotonKeyedRegistry<>(FotonRegistryData.TRIM_PATTERN_KEYS, FotonTrimPattern::new);
    }

    public static Registry<MusicInstrument> instruments() {
        return new FotonKeyedRegistry<>(FotonRegistryData.INSTRUMENT_KEYS, FotonMusicInstrument::new);
    }

    /** The deprecated enum-style lookup: {@code "BASE"} or {@code "minecraft:base"}. */
    public static <T extends Keyed> T valueOf(Registry<T> registry, String name) {
        NamespacedKey key = name == null ? null : NamespacedKey.fromString(name.toLowerCase(Locale.ROOT));
        T value = key == null ? null : registry.get(key);
        if (value == null) throw new IllegalArgumentException("No registry entry found with the name " + name);
        return value;
    }

    /** A data pack {@code description}: a translation, coloured when the pack colours it. */
    static Component describe(String translationKey, String color) {
        Component description = Component.translatable(translationKey);
        TextColor textColor = color == null ? null : TextColor.fromHexString(color);
        return textColor == null ? description : description.color(textColor);
    }
}
