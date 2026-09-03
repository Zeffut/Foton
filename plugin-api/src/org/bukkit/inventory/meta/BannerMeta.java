package org.bukkit.inventory.meta;

import java.util.List;
import org.bukkit.DyeColor;
import org.bukkit.block.banner.Pattern;

public interface BannerMeta extends ItemMeta {
    DyeColor getBaseColor();
    void setBaseColor(DyeColor color);
    List<Pattern> getPatterns();
    void addPattern(Pattern pattern);
    boolean removePattern(int index);
    void setPatterns(List<Pattern> patterns);

    default java.util.Map<String,Object> serialize() { return java.util.Collections.emptyMap(); }
}
