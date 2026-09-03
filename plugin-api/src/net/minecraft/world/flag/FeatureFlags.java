package net.minecraft.world.flag;

/** Feature flag compatibility surface matching vanilla field descriptors. */
public final class FeatureFlags {
    public static final FeatureFlagRegistry REGISTRY = new FeatureFlagRegistry();
    public static final FeatureFlagSet DEFAULT_FLAGS = new FeatureFlagSet();
    private FeatureFlags() {}
}
