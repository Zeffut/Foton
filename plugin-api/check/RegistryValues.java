import org.bukkit.MusicInstrument;
import org.bukkit.NamespacedKey;
import org.bukkit.Registry;
import org.bukkit.block.Biome;
import org.bukkit.block.banner.Pattern;
import org.bukkit.block.banner.PatternType;
import org.bukkit.inventory.meta.trim.TrimMaterial;

/** The registry-backed values Paper made interfaces: generated, unique, and walkable. */
final class RegistryValues {
    private RegistryValues() {}

    static void check() {
        // Plugins compare these with ==, as they did the enums they replace.
        Checks.expect(Registry.BANNER_PATTERN.get(NamespacedKey.minecraft("base")) == PatternType.BASE,
            "a constant and its registry entry are one value");
        Checks.expect(Registry.INSTRUMENT.get(NamespacedKey.minecraft("dream_goat_horn"))
                == MusicInstrument.DREAM_GOAT_HORN,
            "an instrument constant and its registry entry are one value");
        Checks.expect(Registry.BIOME.get(NamespacedKey.fromString("minecraft:plains")) == Biome.PLAINS,
            "a biome resolves by key");
        Checks.expect(Registry.BANNER_PATTERN.get(NamespacedKey.fromString("other:base")) == null,
            "only vanilla entries exist");

        // The pre-1.21 enum surface a plugin written then still calls.
        Checks.same(PatternType.STRIPE_TOP.name(), "STRIPE_TOP", "name is the old constant");
        Checks.expect(PatternType.valueOf("STRIPE_TOP") == PatternType.STRIPE_TOP, "valueOf by name");
        Checks.expect(PatternType.BASE.ordinal() == 0, "the ordinal is the registry order, base first");

        int count = 0;
        for (PatternType ignored : Registry.BANNER_PATTERN) count++;
        Checks.same(count, PatternType.values().length, "the registry walks every pattern");

        // A layer is a value; a design compares layer lists.
        Checks.same(new Pattern(org.bukkit.DyeColor.RED, PatternType.CROSS),
            new Pattern(org.bukkit.DyeColor.RED, PatternType.CROSS), "equal layers are equal");

        // Data the pack gives, not a guess.
        Checks.same(MusicInstrument.PONDER_GOAT_HORN.getRange(), 256.0f, "horn range from the data pack");
        Checks.same(TrimMaterial.AMETHYST.getTranslationKey(), "trim_material.minecraft.amethyst",
            "trim material translation from the data pack");
        Checks.expect(TrimMaterial.AMETHYST.description().color() != null,
            "a trim material description carries the pack's colour");
    }
}
