package foton;

import java.util.UUID;

/** Cow entity handle backed by Steel's living-entity state. */
public final class FotonCow extends FotonAnimals implements org.bukkit.entity.Cow {
    public FotonCow(UUID id) { super(id); }

    @Override public Variant getVariant() {
        String value = Native.cowVariant(getUniqueId().toString());
        if ("minecraft:cold".equals(value)) return Variant.COLD;
        if ("minecraft:warm".equals(value)) return Variant.WARM;
        return Variant.TEMPERATE;
    }

    @Override public void setVariant(Variant variant) {
        java.util.Objects.requireNonNull(variant, "variant");
        Native.setCowVariant(getUniqueId().toString(), variant.getKey().toString());
    }

    @Override public SoundVariant getSoundVariant() {
        String value = Native.cowSoundVariant(getUniqueId().toString());
        return "minecraft:moody".equals(value) ? SoundVariant.MOODY : SoundVariant.CLASSIC;
    }

    @Override public void setSoundVariant(SoundVariant variant) {
        java.util.Objects.requireNonNull(variant, "variant");
        Native.setCowSoundVariant(getUniqueId().toString(), variant.getKey().toString());
    }
}
