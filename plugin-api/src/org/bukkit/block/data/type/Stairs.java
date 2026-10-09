package org.bukkit.block.data.type;

import org.bukkit.block.data.Bisected;
import org.bukkit.block.data.Directional;
import org.bukkit.block.data.Waterlogged;

/** Stair block data and the corner shape its neighbours give it. */
public interface Stairs extends Bisected, Directional, Waterlogged {
    enum Shape { STRAIGHT, INNER_LEFT, INNER_RIGHT, OUTER_LEFT, OUTER_RIGHT }
    Shape getShape();
    void setShape(Shape shape);
}
