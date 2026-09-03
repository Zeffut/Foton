package foton;

import org.bukkit.Location;
import org.bukkit.block.Block;
import org.bukkit.block.Crafter;
import org.bukkit.block.data.BlockData;

/** Live Bukkit facade for a vanilla crafter block entity. */
final class FotonCrafter extends FotonTileState implements Crafter, org.bukkit.inventory.InventoryHolder {
    FotonCrafter(Block block, BlockData data) { super(block, data); }
    @Override public Location getLocation() { return getBlock().getLocation(); }
    @Override public FotonCrafterInventory getInventory() { return new FotonCrafterInventory(this); }
}
