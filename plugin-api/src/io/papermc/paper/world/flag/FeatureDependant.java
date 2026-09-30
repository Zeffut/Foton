package io.papermc.paper.world.flag;

/** API values gated by Minecraft feature flags. */
@org.jspecify.annotations.NullMarked
@org.jetbrains.annotations.ApiStatus.NonExtendable
public interface FeatureDependant {
    default java.util.@org.jetbrains.annotations.Unmodifiable Set<org.bukkit.FeatureFlag>
            requiredFeatures() {
        return java.util.Set.of(org.bukkit.FeatureFlag.VANILLA);
    }
}
