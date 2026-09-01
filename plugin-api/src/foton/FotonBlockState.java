package foton;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.block.data.BlockData;

/** What a block was when it was read.
 *
 * A snapshot, which is Bukkit's contract: changing the world after taking one
 * does not change what it says, and `update` is what writes it back. Plugins
 * read a state, decide, and then either update it or throw it away.
 */
public final class FotonBlockState implements BlockState {
    private final Block block;
    private final BlockData data;

    FotonBlockState(Block block, BlockData data) {
        this.block = block;
        this.data = data;
    }

    @Override
    public Material getType() {
        return data.getMaterial();
    }

    @Override
    public BlockData getBlockData() {
        return data;
    }

    @Override
    public Block getBlock() {
        return block;
    }

    @Override
    public Location getLocation() {
        return block.getLocation();
    }

    @Override
    public World getWorld() {
        return block.getWorld();
    }

    @Override
    public int getX() {
        return block.getX();
    }

    @Override
    public int getY() {
        return block.getY();
    }

    @Override
    public int getZ() {
        return block.getZ();
    }

    @Override
    public boolean update() {
        return update(false);
    }

    @Override
    public boolean update(boolean force) {
        World world = block.getWorld();
        if (world == null) {
            return false;
        }
        Native.setBlock(world.getName(), block.getX(), block.getY(), block.getZ(),
            data.getAsString());
        return true;
    }

    @Override
    public String toString() {
        return "FotonBlockState{" + data.getAsString() + " at " + block + "}";
    }
}
