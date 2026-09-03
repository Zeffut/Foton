package org.bukkit.block.data.type;

import org.bukkit.block.data.Directional;

/** Bed block data. */
public interface Bed extends Directional {
    enum Part { HEAD, FOOT }
    default boolean isOccupied() {
        String state = getAsString();
        return state.contains("occupied=true");
    }
    Part getPart();
    void setPart(Part part);
}
