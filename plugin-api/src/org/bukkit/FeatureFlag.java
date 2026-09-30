package org.bukkit;

/** One vanilla feature-set flag. */
public interface FeatureFlag extends Keyed {
    @org.jetbrains.annotations.ApiStatus.Experimental
    FeatureFlag MINECART_IMPROVEMENTS = FeatureFlags.value("minecart_improvements");
    @org.jetbrains.annotations.ApiStatus.Experimental
    FeatureFlag REDSTONE_EXPERIMENTS = FeatureFlags.value("redstone_experiments");
    @org.jetbrains.annotations.ApiStatus.Experimental
    FeatureFlag TRADE_REBALANCE = FeatureFlags.value("trade_rebalance");
    FeatureFlag VANILLA = FeatureFlags.value("vanilla");
    @Deprecated(since = "1.20")
    FeatureFlag UPDATE_1_20 = FeatureFlags.value("update_1_20");
    @Deprecated(since = "1.21")
    FeatureFlag UPDATE_121 = FeatureFlags.value("update_1_21");
    @Deprecated(since = "1.21.2")
    FeatureFlag BUNDLE = FeatureFlags.value("bundle");
    @Deprecated(since = "1.21.4")
    FeatureFlag WINTER_DROP = FeatureFlags.value("winter_drop");
    net.kyori.adventure.util.Index<net.kyori.adventure.key.Key, FeatureFlag> ALL_FLAGS =
        net.kyori.adventure.util.Index.create(FeatureFlag::key,
            java.util.List.of(MINECART_IMPROVEMENTS, REDSTONE_EXPERIMENTS,
                TRADE_REBALANCE, VANILLA));
}

final class FeatureFlags {
    private FeatureFlags() {}

    static FeatureFlag value(String name) {
        return new Value(NamespacedKey.minecraft(name));
    }

    private record Value(NamespacedKey getKey) implements FeatureFlag {}
}
