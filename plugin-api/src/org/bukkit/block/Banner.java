package org.bukkit.block;

import org.bukkit.DyeColor;
import java.util.List;
import org.bukkit.block.banner.Pattern;

/** A banner block state: its base color and pattern layers, snapshot until updated. */
public interface Banner extends TileState {
    DyeColor getBaseColor();
    void setBaseColor(DyeColor color);
    List<Pattern> getPatterns();
    void setPatterns(List<Pattern> patterns);
    void addPattern(Pattern pattern);
    Pattern getPattern(int i);
    Pattern removePattern(int i);
    void setPattern(int i, Pattern pattern);
    int numberOfPatterns();
    boolean update();
}
