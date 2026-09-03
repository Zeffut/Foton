package org.bukkit.entity;

import org.bukkit.DyeColor;

/** A tropical fish with its vanilla body/pattern colors. */
public interface TropicalFish extends Fish {
    /** Vanilla pattern enum nested in TropicalFish. */
    enum Pattern {
        KOB, SUNSTREAK, SNOOPER, DASHER, BRINELY, SPOTTY,
        FLOPPER, STRIPEY, GLITTER, BLOCKFISH, BETTY, CLAYFISH
    }
    Pattern getPattern();
    void setPattern(Pattern pattern);
    DyeColor getPatternColor();
    void setPatternColor(DyeColor color);
    DyeColor getBodyColor();
    void setBodyColor(DyeColor color);
}
