package org.bukkit.util;

import java.util.Collection;

/** A block's shape as the boxes it is made of, in block-local coordinates. */
public interface VoxelShape {
    Collection<BoundingBox> getBoundingBoxes();

    /** Whether any box of the shape overlaps {@code other}. */
    boolean overlaps(BoundingBox other);
}
