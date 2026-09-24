package foton;

import net.kyori.adventure.text.Component;
import org.bukkit.inventory.meta.trim.TrimPattern;

/** One vanilla armor trim pattern, with the description the data pack gives it. */
public final class FotonTrimPattern extends FotonOldEnum implements TrimPattern {
    private final String translationKey;
    private final String color;
    private final boolean decal;

    FotonTrimPattern(String path, int ordinal) {
        super(path, ordinal);
        this.translationKey = FotonRegistryData.TRIM_PATTERN_DESCRIPTIONS[ordinal];
        this.color = FotonRegistryData.TRIM_PATTERN_COLORS[ordinal];
        this.decal = FotonRegistryData.TRIM_PATTERN_DECALS[ordinal];
    }

    @Override
    public Component description() {
        return FotonRegistries.describe(translationKey, color);
    }

    @Override
    public String getTranslationKey() {
        return translationKey;
    }

    /** Whether the pattern is drawn as a decal over the armor rather than dyed into it. */
    public boolean decal() {
        return decal;
    }
}
