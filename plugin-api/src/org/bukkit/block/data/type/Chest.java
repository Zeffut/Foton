package org.bukkit.block.data.type;

import org.bukkit.block.data.Directional;

/** Chest block data and its vanilla double-chest half. */
public interface Chest extends Directional {
    enum Type { SINGLE, LEFT, RIGHT }
    Type getType();
    void setType(Type type);
}
