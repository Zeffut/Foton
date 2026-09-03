package org.bukkit.block;

import org.bukkit.DyeColor;
import java.util.List;
import org.bukkit.block.banner.Pattern;

/** A banner block state backed by its live block entity. */
public interface Banner extends TileState {
    DyeColor getBaseColor();
    void setBaseColor(DyeColor color);
    List<Pattern> getPatterns();
    void addPattern(Pattern pattern);
    boolean update();
}
