package org.bukkit.entity;

import org.jetbrains.annotations.NotNull;

/** A mooshroom cow. */
public interface MushroomCow extends AbstractCow, io.papermc.paper.entity.Shearable {
    boolean hasEffectsForNextStew();
    @NotNull
    java.util.List<org.bukkit.potion.PotionEffect> getEffectsForNextStew();
    @Deprecated(since = "1.20.2", forRemoval = true)
    boolean addEffectToNextStew(@NotNull org.bukkit.potion.PotionEffect effect, boolean overwrite);
    boolean addEffectToNextStew(
        @NotNull io.papermc.paper.potion.SuspiciousEffectEntry suspiciousEffectEntry, boolean overwrite);
    boolean removeEffectFromNextStew(@NotNull org.bukkit.potion.PotionEffectType type);
    boolean hasEffectForNextStew(@NotNull org.bukkit.potion.PotionEffectType type);
    void clearEffectsForNextStew();
    enum Variant { RED, BROWN }
    @NotNull
    Variant getVariant();
    void setVariant(@NotNull Variant variant);

    @Deprecated(since = "1.20.2", forRemoval = true)
    @org.jetbrains.annotations.Contract("-> fail")
    default int getStewEffectDuration() {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #getStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    @org.jetbrains.annotations.Contract("_ -> fail")
    default void setStewEffectDuration(int duration) {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #setStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    @org.jetbrains.annotations.Contract("-> fail")
    default org.bukkit.potion.PotionEffectType getStewEffectType() {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #getStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    @org.jetbrains.annotations.Contract("_ -> fail")
    default void setStewEffect(@org.jetbrains.annotations.Nullable org.bukkit.potion.PotionEffectType type) {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #setStewEffects");
    }
    java.util.@NotNull @org.jetbrains.annotations.Unmodifiable List<io.papermc.paper.potion.SuspiciousEffectEntry> getStewEffects();
    void setStewEffects(java.util.@NotNull List<io.papermc.paper.potion.SuspiciousEffectEntry> effects);
}
