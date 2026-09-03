package org.bukkit.block.data.type;
import org.bukkit.block.data.Directional;
/** Vanilla rail data contract. */
public interface Rail extends org.bukkit.block.data.BlockData {
    enum Shape { STRAIGHT, ASCENDING_EAST, ASCENDING_WEST, ASCENDING_NORTH, ASCENDING_SOUTH, SOUTH_EAST, SOUTH_WEST, NORTH_WEST, NORTH_EAST }
    Shape getShape(); void setShape(Shape shape); boolean isWaterlogged(); void setWaterlogged(boolean waterlogged);
}
