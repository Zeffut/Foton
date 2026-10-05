package org.bukkit.util;

import java.util.Collection;

/** A block's shape as the boxes it is made of, in block-local coordinates. */
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
