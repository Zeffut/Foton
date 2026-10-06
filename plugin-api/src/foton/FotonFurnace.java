package foton;

import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.inventory.FurnaceInventory;

/** A furnace, smoker or blast furnace snapshot. Its inventory is live, as a
 * Bukkit block state's is; its times are the snapshot's own. Each block gets
 * its own interface, so a plain furnace is not a {@code BlastFurnace}. */
class FotonFurnace extends FotonTileState implements org.bukkit.block.Furnace {
    static FotonFurnace of(Block block, BlockData data) {
        return switch (block.getType()) {
            case BLAST_FURNACE -> new Blast(block, data);
            case SMOKER -> new Smoking(block, data);
            default -> new FotonFurnace(block, data);
        };
    }

    static final class Blast extends FotonFurnace implements org.bukkit.block.BlastFurnace {
        Blast(Block block, BlockData data) { super(block, data); }
    }

    static final class Smoking extends FotonFurnace implements org.bukkit.block.Smoker {
        Smoking(Block block, BlockData data) { super(block, data); }
    }

    private short burnTime;
    private short cookTime;
    private int cookTimeTotal;

    FotonFurnace(Block block, BlockData data) {
        super(block, data);
        int[] times = block == null || block.getWorld() == null ? null
            : Native.furnaceTimes(block.getWorld().getName(), block.getX(), block.getY(), block.getZ());
        if (times != null && times.length == 3) {
            burnTime = (short) times[0];
            cookTime = (short) times[1];
            cookTimeTotal = times[2];
        }
    }

    @Override public Location getLocation() { return getBlock().getLocation(); }
    @Override public short getBurnTime() { return burnTime; }
    @Override public void setBurnTime(short burnTime) { this.burnTime = burnTime; }
    @Override public short getCookTime() { return cookTime; }
    @Override public void setCookTime(short cookTime) { this.cookTime = cookTime; }
    @Override public int getCookTimeTotal() { return cookTimeTotal; }
    @Override public void setCookTimeTotal(int cookTimeTotal) { this.cookTimeTotal = cookTimeTotal; }
    @Override public FurnaceInventory getInventory() { return new FotonFurnaceInventory(this); }
    @Override public FurnaceInventory getSnapshotInventory() { return getInventory(); }

    @Override
    public boolean update(boolean force) {
        if (!super.update(force)) return false;
        Block block = getBlock();
        Native.setFurnaceTimes(block.getWorld().getName(), block.getX(), block.getY(), block.getZ(),
            new int[] {burnTime, cookTime, cookTimeTotal});
        return true;
    }
}
