package io.papermc.paper.world.flag;

/** API values gated by Minecraft feature flags. */
public interface FeatureDependant {
    default java.util.Set<org.bukkit.FeatureFlag> requiredFeatures() {
        return java.util.Set.of(org.bukkit.FeatureFlag.VANILLA);
    }
}
