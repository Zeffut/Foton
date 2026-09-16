package io.papermc.paper.potion;

/** A suspicious-stew effect type and duration. */
public sealed interface SuspiciousEffectEntry permits SuspiciousEffectEntryImpl {
    org.bukkit.potion.PotionEffectType effect();
    int duration();
    static SuspiciousEffectEntry create(org.bukkit.potion.PotionEffectType effectType, int duration) {
        return new SuspiciousEffectEntryImpl(effectType, duration);
    }
}

record SuspiciousEffectEntryImpl(
    org.bukkit.potion.PotionEffectType effect,
    int duration
) implements SuspiciousEffectEntry { }
