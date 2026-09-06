package org.bukkit.event.block;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;

/** Common state for piston extension and retraction events.
 *
 * <p>Sits under {@link BlockPistonEvent}, which is Bukkit's chain: a listener
 * registered for the piston base -- or for `BlockEvent` above it -- has to hear
 * an extend and a retract. Parenting this straight to `Event` cut both off, so
 * a plugin watching blocks saw every other block change and no piston.
 *
 * <p>The list of moved blocks lives here rather than on the parent because it
 * is what the two subclasses share and Bukkit's base does not carry.
 */
public abstract class PistonEvent extends BlockPistonEvent {
    private final List<Block> blocks;

    protected PistonEvent(Block block, BlockFace direction, List<Block> blocks) {
        super(block, direction);
        this.blocks = new ArrayList<>(blocks);
    }

    /** The blocks the piston is about to move. */
    public List<Block> getBlocks() { return blocks; }
}
