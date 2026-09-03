package foton;

import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.ShulkerBox;
import org.bukkit.block.data.BlockData;

/** In-memory shulker block-state snapshot carried by item metadata. */
public final class FotonShulkerBox implements ShulkerBox, org.bukkit.inventory.InventoryHolder {
    public FotonShulkerBox() { data = new org.bukkit.block.data.SimpleBlockData("minecraft:shulker_box"); }
    private BlockData data;
    private final FotonShulkerInventory inventory = new FotonShulkerInventory(this);
    @Override public org.bukkit.inventory.Inventory getInventory() { return inventory; }
    @Override public org.bukkit.inventory.Inventory getSnapshotInventory() { return inventory.snapshot(); }
    @Override public Material getType() { return Material.SHULKER_BOX; }
    @Override public BlockData getBlockData() { return data == null ? null : data.clone(); }
    @Override public void setBlockData(BlockData value) { data = value == null ? null : value.clone(); }
    @Override public Block getBlock() { return null; }
    @Override public Location getLocation() { return null; }
    @Override public World getWorld() { return null; }
    @Override public int getX() { return 0; } @Override public int getY() { return 0; } @Override public int getZ() { return 0; }
    @Override public boolean update() { return false; } @Override public boolean update(boolean force) { return false; }
}
