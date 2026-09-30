package io.papermc.paper.datacomponent.item;

import io.papermc.paper.datacomponent.DataComponentBuilder;
import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.banner.Pattern;

/** The {@code banner_patterns} component: a banner's or shield's layers, bottom first. */
public interface BannerPatternLayers {
    static BannerPatternLayers bannerPatternLayers(List<Pattern> patterns) {
        return bannerPatternLayers().addAll(patterns).build();
    }

    static Builder bannerPatternLayers() {
        return new Builder() {
            private final List<Pattern> patterns = new ArrayList<>();

            @Override
            public Builder add(Pattern pattern) {
                patterns.add(java.util.Objects.requireNonNull(pattern, "pattern"));
                return this;
            }

            @Override
            public Builder addAll(List<Pattern> values) {
                for (Pattern pattern : values) add(pattern);
                return this;
            }

            @Override
            public BannerPatternLayers build() {
                return new Layers(List.copyOf(patterns));
            }
        };
    }

    List<Pattern> patterns();

    interface Builder extends DataComponentBuilder<BannerPatternLayers> {
        Builder add(Pattern pattern);

        Builder addAll(List<Pattern> patterns);
    }

    /** The one implementation: an immutable list of layers. */
    record Layers(List<Pattern> patterns) implements BannerPatternLayers { }
}
