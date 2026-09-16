package org.bukkit.potion;

/**
 * Legacy base for potion-effect type wrappers.
 *
 * @deprecated registry values are exposed directly
 */
@Deprecated(since = "1.20.3", forRemoval = true)
public abstract class PotionEffectTypeWrapper extends PotionEffectType {
    protected PotionEffectTypeWrapper() {}

    @org.jetbrains.annotations.NotNull
    public PotionEffectType getType() {
        return this;
    }
}
