package net.minecraft.commands;

import net.minecraft.core.HolderLookup;
import net.minecraft.world.flag.FeatureFlagSet;

/** Vanilla command build context marker. */
public interface CommandBuildContext extends HolderLookup.Provider {
    static CommandBuildContext simple(HolderLookup.Provider access, FeatureFlagSet enabledFeatures) {
        return new CommandBuildContext() { public FeatureFlagSet enabledFeatures() { return enabledFeatures; } };
    }
    FeatureFlagSet enabledFeatures();
}
