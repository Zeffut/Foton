package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.DyeColor;
import org.bukkit.block.banner.Pattern;

public final class SimpleBannerMeta extends SimpleItemMeta implements BannerMeta {
    private DyeColor baseColor = DyeColor.WHITE;
    @Override public DyeColor getBaseColor() { return baseColor; }
    @Override public void setBaseColor(DyeColor value) { baseColor = value == null ? DyeColor.WHITE : value; }
    // A banner item's layers are its banner_patterns component, which every
    // meta carries (a shield's too).
    @Override public List<Pattern> getPatterns() {
        List<Pattern> patterns = getBannerPatternsComponent();
        return patterns == null ? List.of() : patterns;
    }
    @Override public void addPattern(Pattern pattern) {
        if (pattern == null) return;
        List<Pattern> patterns = new ArrayList<>(getPatterns());
        patterns.add(pattern);
        setBannerPatternsComponent(patterns);
    }
    @Override public boolean removePattern(int index) {
        List<Pattern> patterns = new ArrayList<>(getPatterns());
        if (index < 0 || index >= patterns.size()) return false;
        patterns.remove(index);
        setBannerPatternsComponent(patterns.isEmpty() ? null : patterns);
        return true;
    }
    @Override public void setPatterns(List<Pattern> values) {
        setBannerPatternsComponent(values == null || values.isEmpty() ? null : values);
    }
    @Override public SimpleBannerMeta clone() { return (SimpleBannerMeta) super.clone(); }
    @Override public boolean equals(Object other) {
        return other instanceof SimpleBannerMeta meta && super.equals(other) && baseColor == meta.baseColor;
    }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), baseColor); }
}
