package org.bukkit;

/** One vanilla feature-set flag. */
public interface FeatureFlag extends Keyed {
    FeatureFlag MINECART_IMPROVEMENTS = FeatureFlags.value("minecart_improvements");
    FeatureFlag REDSTONE_EXPERIMENTS = FeatureFlags.value("redstone_experiments");
    FeatureFlag TRADE_REBALANCE = FeatureFlags.value("trade_rebalance");
    FeatureFlag VANILLA = FeatureFlags.value("vanilla");
    FeatureFlag UPDATE_1_20 = FeatureFlags.value("update_1_20");
    FeatureFlag UPDATE_121 = FeatureFlags.value("update_1_21");
    FeatureFlag BUNDLE = FeatureFlags.value("bundle");
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
