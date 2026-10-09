package org.bukkit.block.data;

import java.util.Set;
import org.bukkit.Axis;

/** Block data with an {@code axis}: logs, pillars, chains, portals. */
public interface Orientable extends BlockData {
    Axis getAxis();
    void setAxis(Axis axis);
    Set<Axis> getAxes();
}
