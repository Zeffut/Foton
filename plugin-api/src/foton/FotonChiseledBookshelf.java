package foton;

import org.bukkit.block.Block;
import org.bukkit.block.ChiseledBookshelf;
import org.bukkit.block.data.BlockData;

/** Live chiseled bookshelf state. */
final class FotonChiseledBookshelf extends FotonTileState implements ChiseledBookshelf {
    FotonChiseledBookshelf(Block block, BlockData data) { super(block, data); }
    @Override public FotonChiseledBookshelfInventory getInventory() { return new FotonChiseledBookshelfInventory(this); }
}
