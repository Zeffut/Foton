package foton;

import java.io.IOException;
import java.util.Arrays;
import org.bukkit.block.Block;
import org.bukkit.block.TileState;
import org.bukkit.block.data.BlockData;

/** Base snapshot for a block that has a vanilla block entity.
 *
 * <p>Its persistent data is the block entity's {@code PublicBukkitValues},
 * read when the snapshot is taken and written back by {@code update}, which is
 * where Paper keeps it too. */
public class FotonTileState extends FotonBlockState implements TileState {
    private byte[] loaded;

    protected FotonTileState(Block block, BlockData data) {
        this(block, data, block == null || block.getWorld() == null ? null
            : Native.blockEntityPersistentData(
                block.getWorld().getName(), block.getX(), block.getY(), block.getZ()));
    }

    /** A snapshot whose block entity data was already fetched; null when none stands there. */
    FotonTileState(Block block, BlockData data, byte[] stored) {
        super(block, data);
        if (stored == null) return;
        try {
            persistentData.readFromBytes(stored, true);
            loaded = persistentData.serializeToBytes();
        } catch (IOException unreadable) {
            // Data this API cannot read stays on the block entity untouched.
            loaded = null;
        }
    }

    @Override
    public boolean update(boolean force) {
        return super.update(force) && writePersistentData();
    }

    /** Writes the snapshot's persistent data back when it differs from what was
     * read; false when the block entity refused it. */
    private boolean writePersistentData() {
        Block block = getBlock();
        if (loaded == null || block == null || block.getWorld() == null) return true;
        try {
            byte[] now = persistentData.serializeToBytes();
            if (Arrays.equals(now, loaded)) return true;
            if (!Native.setBlockEntityPersistentData(
                    block.getWorld().getName(), block.getX(), block.getY(), block.getZ(), now)) {
                return false;
            }
            loaded = now;
            return true;
        } catch (IOException unwritable) {
            // A value that cannot be written leaves the block entity as it was.
            return false;
        }
    }
}
