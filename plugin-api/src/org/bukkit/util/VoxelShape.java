package org.bukkit.util;

import java.util.Collection;

/** A block collision shape represented by its constituent axis-aligned boxes. */
public interface VoxelShape {
    Collection<BoundingBox> getBoundingBoxes();

    default boolean overlaps(BoundingBox other) {
        if (other == null) return false;
        for (BoundingBox box : getBoundingBoxes()) {
            if (box != null && box.overlaps(other)) return true;
        }
        return false;
    }
}
