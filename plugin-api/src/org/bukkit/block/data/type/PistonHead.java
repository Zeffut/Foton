package org.bukkit.block.data.type;

/** Directional piston head data. */
public interface PistonHead extends TechnicalPiston {
    boolean isShort();
    void setShort(boolean shortHead);
}
