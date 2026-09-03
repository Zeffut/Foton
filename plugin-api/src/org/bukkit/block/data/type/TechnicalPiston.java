package org.bukkit.block.data.type;
import org.bukkit.block.data.Directional;
/** Directional technical piston data. */
public interface TechnicalPiston extends Directional {
    enum Type { NORMAL, STICKY }
    default Type getType() { return Type.NORMAL; }
}
