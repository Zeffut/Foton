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

    public BlockFace getOppositeFace() {
        return switch (this) {
            case NORTH -> SOUTH;
            case EAST -> WEST;
            case SOUTH -> NORTH;
            case WEST -> EAST;
            case UP -> DOWN;
            case DOWN -> UP;
            case NORTH_EAST -> SOUTH_WEST;
            case NORTH_WEST -> SOUTH_EAST;
            case SOUTH_EAST -> NORTH_WEST;
            case SOUTH_WEST -> NORTH_EAST;
            case SELF -> SELF;
        };
    }
}
