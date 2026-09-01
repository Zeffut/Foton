package foton;

import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.block.Block;

/** A block, as a plugin holds one: a world and three coordinates. */
public final class FotonBlock implements Block {
    private final World world;
    private final int x;
    private final int y;
    private final int z;

    public FotonBlock(World world, int x, int y, int z) {
        this.world = world;
        this.x = x;
        this.y = y;
        this.z = z;
    }

    @Override
    public int getX() {
        return x;
    }

    @Override
    public int getY() {
        return y;
    }

    @Override
    public int getZ() {
        return z;
    }

    @Override
    public World getWorld() {
        return world;
    }

    @Override
    public Location getLocation() {
        return new Location(world, x, y, z);
    }

    @Override
    public boolean equals(Object other) {
        return other instanceof FotonBlock block
            && x == block.x && y == block.y && z == block.z
            && java.util.Objects.equals(world, block.world);
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(world, x, y, z);
    }

    @Override
    public String toString() {
        return "FotonBlock{" + (world == null ? "?" : world.getName())
            + " " + x + ", " + y + ", " + z + "}";
    }
}
