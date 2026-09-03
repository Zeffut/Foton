package org.bukkit.block.data;

/** Rail shape and waterlogging properties. */
public interface Rail extends BlockData {
    enum Shape { STRAIGHT, ASCENDING_EAST, ASCENDING_WEST, ASCENDING_NORTH, ASCENDING_SOUTH, SOUTH_EAST, SOUTH_WEST, NORTH_WEST, NORTH_EAST }
    Shape getShape();
    void setShape(Shape shape);
    boolean isWaterlogged();
    void setWaterlogged(boolean waterlogged);
}
