package org.bukkit.entity;

/** A mooshroom cow. */
public interface MushroomCow extends AbstractCow, io.papermc.paper.entity.Shearable {
    boolean hasEffectsForNextStew();
    java.util.List<org.bukkit.potion.PotionEffect> getEffectsForNextStew();
    @Deprecated(since = "1.20.2", forRemoval = true)
    boolean addEffectToNextStew(org.bukkit.potion.PotionEffect effect, boolean overwrite);
    boolean addEffectToNextStew(
        io.papermc.paper.potion.SuspiciousEffectEntry suspiciousEffectEntry, boolean overwrite);
    boolean removeEffectFromNextStew(org.bukkit.potion.PotionEffectType type);
    boolean hasEffectForNextStew(org.bukkit.potion.PotionEffectType type);
    void clearEffectsForNextStew();
    enum Variant { RED, BROWN }
    Variant getVariant();
    void setVariant(Variant variant);

    @Deprecated(since = "1.20.2", forRemoval = true)
    default int getStewEffectDuration() {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #getStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    default void setStewEffectDuration(int duration) {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #setStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    default org.bukkit.potion.PotionEffectType getStewEffectType() {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #getStewEffects");
    }
    @Deprecated(since = "1.20.2", forRemoval = true)
    default void setStewEffect(org.bukkit.potion.PotionEffectType type) {
        throw new UnsupportedOperationException(
            "Mushroom cows can now hold multiple effects. Use #setStewEffects");
    }
    java.util.List<io.papermc.paper.potion.SuspiciousEffectEntry> getStewEffects();
    void setStewEffects(java.util.List<io.papermc.paper.potion.SuspiciousEffectEntry> effects);
}
