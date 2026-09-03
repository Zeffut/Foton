package foton;

import org.bukkit.block.Block;
import org.bukkit.block.TileState;
import org.bukkit.block.data.BlockData;

/** Base snapshot for a block that has a vanilla block entity. */
public abstract class FotonTileState extends FotonBlockState implements TileState {
    protected FotonTileState(Block block, BlockData data) {
        super(block, data);
    }
}
