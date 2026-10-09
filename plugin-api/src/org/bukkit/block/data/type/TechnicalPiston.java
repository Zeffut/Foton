package org.bukkit.block.data.type;

import org.bukkit.block.data.Directional;

/** Directional technical piston data. */
public interface TechnicalPiston extends Directional {
    enum Type { NORMAL, STICKY }
    Type getType();
    void setType(Type type);
}
