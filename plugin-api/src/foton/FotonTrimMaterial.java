package foton;

import net.kyori.adventure.text.Component;
import org.bukkit.inventory.meta.trim.TrimMaterial;

/** One vanilla armor trim material, with the description the data pack gives it. */
public final class FotonTrimMaterial extends FotonOldEnum implements TrimMaterial {
    private final String translationKey;
    private final String color;

    FotonTrimMaterial(String path, int ordinal) {
        super(path, ordinal);
        this.translationKey = FotonRegistryData.TRIM_MATERIAL_DESCRIPTIONS[ordinal];
        this.color = FotonRegistryData.TRIM_MATERIAL_COLORS[ordinal];
    }

    @Override
    public Component description() {
        return FotonRegistries.describe(translationKey, color);
    }

    @Override
    public String getTranslationKey() {
        return translationKey;
    }
}
