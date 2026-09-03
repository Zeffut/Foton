package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.DyeColor;
import org.bukkit.block.banner.Pattern;

public final class SimpleBannerMeta extends SimpleItemMeta implements BannerMeta {
    private DyeColor baseColor = DyeColor.WHITE;
    private List<Pattern> patterns = new ArrayList<>();
    @Override public DyeColor getBaseColor() { return baseColor; }
    @Override public void setBaseColor(DyeColor value) { baseColor = value == null ? DyeColor.WHITE : value; }
    @Override public List<Pattern> getPatterns() { return List.copyOf(patterns); }
    @Override public void addPattern(Pattern pattern) { if (pattern != null) patterns.add(pattern); }
    @Override public boolean removePattern(int index) { if (index < 0 || index >= patterns.size()) return false; patterns.remove(index); return true; }
    @Override public void setPatterns(List<Pattern> values) { patterns = values == null ? new ArrayList<>() : new ArrayList<>(values); }
        @Override public SimpleBannerMeta clone() { SimpleBannerMeta copy = (SimpleBannerMeta) super.clone(); copy.patterns = new ArrayList<>(patterns); return copy; }
    @Override public boolean equals(Object other) {
        return other instanceof SimpleBannerMeta meta && super.equals(other)
            && baseColor == meta.baseColor && patterns.equals(meta.patterns);
    }
    @Override public int hashCode() { return java.util.Objects.hash(super.hashCode(), baseColor, patterns); }
}
