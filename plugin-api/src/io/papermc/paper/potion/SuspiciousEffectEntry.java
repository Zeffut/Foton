package io.papermc.paper.potion;

import org.jetbrains.annotations.ApiStatus;
import org.jetbrains.annotations.Contract;
import org.jspecify.annotations.NullMarked;

/** A suspicious-stew effect type and duration. */
@NullMarked
public sealed interface SuspiciousEffectEntry permits SuspiciousEffectEntryImpl {
    org.bukkit.potion.PotionEffectType effect();
    int duration();
    @Contract(value = "_, _ -> new", pure = true)
    static SuspiciousEffectEntry create(org.bukkit.potion.PotionEffectType effectType, int duration) {
        return new SuspiciousEffectEntryImpl(effectType, duration);
    }
}

@ApiStatus.Internal
@NullMarked
record SuspiciousEffectEntryImpl(
    org.bukkit.potion.PotionEffectType effect,
    int duration
) implements SuspiciousEffectEntry { }
