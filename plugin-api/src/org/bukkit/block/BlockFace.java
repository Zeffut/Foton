package org.bukkit.block;

/** Which way from a block.
 *
 * The offsets are the ones vanilla uses, so `getRelative` on a Foton block
 * lands where a plugin written against Bukkit expects it to.
 */
public enum BlockFace {
    NORTH(0, 0, -1),
    EAST(1, 0, 0),
    SOUTH(0, 0, 1),
    WEST(-1, 0, 0),
    UP(0, 1, 0),
    DOWN(0, -1, 0),
    NORTH_EAST(1, 0, -1),
    NORTH_NORTH_EAST(1, 0, -2),
    EAST_NORTH_EAST(2, 0, -1),
    EAST_SOUTH_EAST(2, 0, 1),
    SOUTH_SOUTH_EAST(1, 0, 2),
    SOUTH_SOUTH_WEST(-1, 0, 2),
    WEST_SOUTH_WEST(-2, 0, 1),
    WEST_NORTH_WEST(-2, 0, -1),
    NORTH_NORTH_WEST(-1, 0, -2),
    NORTH_WEST(-1, 0, -1),
    SOUTH_EAST(1, 0, 1),
    SOUTH_WEST(-1, 0, 1),
    SELF(0, 0, 0);

    private final int x;
    private final int y;
    private final int z;

    BlockFace(int x, int y, int z) {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public int getModX() {
        return x;
    }

    public int getModY() {
        return y;
    }

    public int getModZ() {
        return z;
    }

    /** Unit vector pointing along this face, matching Bukkit's diagonal normalization. */
    public org.bukkit.util.Vector getDirection() {
        double length = Math.sqrt((double) x * x + (double) y * y + (double) z * z);
        return length == 0.0 ? new org.bukkit.util.Vector() : new org.bukkit.util.Vector(x / length, y / length, z / length);
    }

    public BlockFace getOppositeFace() {
        return switch (this) {
            case NORTH -> SOUTH;
            case EAST -> WEST;
            case SOUTH -> NORTH;
            case WEST -> EAST;
            case UP -> DOWN;
            case DOWN -> UP;
            case NORTH_EAST -> SOUTH_WEST;
            case NORTH_NORTH_EAST -> SOUTH_SOUTH_WEST;
            case EAST_NORTH_EAST -> WEST_SOUTH_WEST;
            case EAST_SOUTH_EAST -> WEST_NORTH_WEST;
            case SOUTH_SOUTH_EAST -> NORTH_NORTH_WEST;
            case SOUTH_SOUTH_WEST -> NORTH_NORTH_EAST;
            case WEST_SOUTH_WEST -> EAST_NORTH_EAST;
            case WEST_NORTH_WEST -> EAST_SOUTH_EAST;
            case NORTH_WEST -> SOUTH_EAST;
            case SOUTH_EAST -> NORTH_WEST;
            case SOUTH_WEST -> NORTH_EAST;
            case SELF -> SELF;
            default -> SELF;
        };
    }
}
