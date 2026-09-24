package foton;

import org.bukkit.block.Biome;

/** One vanilla biome. */
public final class FotonBiome extends FotonOldEnum implements Biome {
    FotonBiome(String path, int ordinal) {
        super(path, ordinal);
    }

    /** Paper's default: {@code biome.minecraft.<path>}. */
    @Override
    public String translationKey() {
        return "biome." + getKey().getNamespace() + "." + getKey().getKey();
    }

    @Override
    public int compareTo(Biome other) {
        return compareOrdinal((FotonOldEnum) other);
    }
}
