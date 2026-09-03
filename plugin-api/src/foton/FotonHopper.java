package foton;

import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.Hopper;
import org.bukkit.block.data.BlockData;

final class FotonHopper extends FotonTileState implements Hopper, org.bukkit.inventory.InventoryHolder {
    FotonHopper(Block block, BlockData data) { super(block, data); }
    @Override public Location getLocation() { return getBlock().getLocation(); }
    @Override public FotonHopperInventory getInventory() { return new FotonHopperInventory(this, 5); }
    @Override public String getCustomName() { return Native.hopperCustomName(getWorld().getName(), getX(), getY(), getZ()); }
    @Override public void setCustomName(String name) { Native.hopperSetCustomName(getWorld().getName(), getX(), getY(), getZ(), name == null ? "" : name); }
}
