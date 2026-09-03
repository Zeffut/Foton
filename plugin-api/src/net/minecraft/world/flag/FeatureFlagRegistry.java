package net.minecraft.world.flag;

/** Registry marker used by the vanilla feature-flag API. */
public class FeatureFlagRegistry {
    public FeatureFlagSet allFlags() { return new FeatureFlagSet(); }
}
