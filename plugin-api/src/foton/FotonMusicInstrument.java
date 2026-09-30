package foton;

import net.kyori.adventure.text.Component;
import org.bukkit.MusicInstrument;
import org.bukkit.NamespacedKey;
import org.bukkit.Sound;

/** One vanilla goat horn sound, with the duration, range and sound the data pack gives it. */
public final class FotonMusicInstrument extends MusicInstrument {
    private final NamespacedKey key;
    private final int ordinal;

    FotonMusicInstrument(String path, int ordinal) {
        this.key = NamespacedKey.minecraft(path);
        this.ordinal = ordinal;
    }

    @Override
    public float getDuration() {
        return FotonRegistryData.INSTRUMENT_DURATIONS[ordinal];
    }

    @Override
    public float getRange() {
        return FotonRegistryData.INSTRUMENT_RANGES[ordinal];
    }

    @Override
    public Component description() {
        return FotonRegistries.describe(FotonRegistryData.INSTRUMENT_DESCRIPTIONS[ordinal],
            FotonRegistryData.INSTRUMENT_COLORS[ordinal]);
    }

    /** The horn's sound event, or null if the Bukkit sound enum has no constant for it. */
    @Override
    public Sound getSound() {
        NamespacedKey sound = NamespacedKey.fromString(FotonRegistryData.INSTRUMENT_SOUNDS[ordinal]);
        for (Sound value : Sound.values()) {
            if (value.getKey().equals(sound)) return value;
        }
        return null;
    }

    @Override
    public NamespacedKey getKey() {
        return key;
    }

    @Override
    public String translationKey() {
        return FotonRegistryData.INSTRUMENT_DESCRIPTIONS[ordinal];
    }

    @Override
    public String toString() {
        return key.toString();
    }
}
