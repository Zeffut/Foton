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
public class FotonBlockState implements BlockState {
    private final Block block;
    private String originalState;
    private BlockData data;
    private final FotonPersistentDataContainer persistentData = new FotonPersistentDataContainer();

    /** A state that belongs to no world yet.
     *
     * <p>Bukkit's {@code BlockData.createBlockState}: a shape a plugin builds
     * to stamp somewhere later. Everything positional answers null or zero
     * until it is placed, and {@code update} has nothing to write back to. */
    public static FotonBlockState detached(BlockData data) {
        return new FotonBlockState(null, data);
    }

    protected FotonBlockState(Block block, BlockData data) {
        this.block = block;
        this.data = data;
        this.originalState = data == null ? null : data.getAsString();
    }
    @Override public org.bukkit.persistence.PersistentDataContainer getPersistentDataContainer() {
        return persistentData;
    }

    @Override
    public Material getType() {
        return data.getMaterial();
    }

    @Override
    public void setBlockData(BlockData value) {
        if (value != null) data = value.clone();
    }

    @Override
    public BlockData getBlockData() {
        return data == null ? null : data.clone();
    }

    @Override
    public Block getBlock() {
        return block;
    }

    @Override
    public Location getLocation() {
        return block == null ? null : block.getLocation();
    }

    @Override
    public World getWorld() {
        return block == null ? null : block.getWorld();
    }

    @Override
    public int getX() {
        return block == null ? 0 : block.getX();
    }

    @Override
    public int getY() {
        return block == null ? 0 : block.getY();
    }

    @Override
    public int getZ() {
        return block == null ? 0 : block.getZ();
    }

    @Override
    public boolean update() {
        return update(false);
    }

    @Override
    public boolean update(boolean force) {
        if (block == null) {
            return false;
        }
        World world = block.getWorld();
        if (world == null || data == null) {
            return false;
        }
        if (!force) {
            String current = Native.blockState(world.getName(), block.getX(), block.getY(), block.getZ());
            if (current == null || !current.equals(originalState)) {
                return false;
            }
        }
        String state = data.getAsString();
        Native.setBlock(world.getName(), block.getX(), block.getY(), block.getZ(), state);
        originalState = state;
        return true;
    }

    @Override
    public String toString() {
        return "FotonBlockState{" + (data == null ? "null" : data.getAsString())
                + (block == null ? " (detached)" : " at " + block) + "}";
    }
}
