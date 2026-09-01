package foton;

import org.bukkit.World;
import org.bukkit.block.Block;

/** A block a plugin was handed, as coordinates and a world. */
public final class FotonBlock implements Block {
    private final int x;
    private final int y;
    private final int z;
    private final String world;

    public FotonBlock(int x, int y, int z, String world) {
        this.x = x;
        this.y = y;
        this.z = z;
        this.world = world;
    }

    @Override public int getX() { return x; }
    @Override public int getY() { return y; }
    @Override public int getZ() { return z; }
    @Override public World getWorld() { return new FotonWorld(world); }
}
