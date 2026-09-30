package foton;

import org.bukkit.block.banner.PatternType;

/** One vanilla banner pattern. */
public final class FotonPatternType extends FotonOldEnum implements PatternType {
    private final String translationKey;

    FotonPatternType(String path, int ordinal) {
        super(path, ordinal);
        this.translationKey = FotonRegistryData.BANNER_PATTERN_TRANSLATION_KEYS[ordinal];
    }

    /** The namespaced key; see {@link PatternType#getIdentifier()} for why not Paper's short code. */
    @Override
    public String getIdentifier() {
        return getKey().toString();
    }

    /** The client's name for the pattern, before the colour is prefixed to it. */
    public String translationKey() {
        return translationKey;
    }

    @Override
    public int compareTo(PatternType other) {
        return compareOrdinal((FotonOldEnum) other);
    }
}
