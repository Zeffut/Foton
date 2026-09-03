package foton;

import org.bukkit.block.Block;
import org.bukkit.block.data.BlockData;
import org.bukkit.block.Dispenser;
import org.bukkit.inventory.Inventory;

/** Bukkit view of a dispenser block state. */
final class FotonDispenser extends FotonTileState implements Dispenser, org.bukkit.inventory.InventoryHolder {
    FotonDispenser(Block block, BlockData data) { super(block, data); }
    @Override public Inventory getInventory() { return new FotonHopperInventory(this, 9); }
    @Override public Inventory getSnapshotInventory() {
        FotonCustomInventory snapshot = new FotonCustomInventory(this, 9, "Dispenser");
        snapshot.setContents(getInventory().getContents());
        return snapshot;
    }
    @Override public boolean update(boolean force, boolean applyPhysics) { return update(force); }
}
