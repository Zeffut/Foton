package foton;

import org.bukkit.block.Lectern;
import org.bukkit.block.data.BlockData;
import org.bukkit.inventory.Inventory;

/** Live lectern state backed by the server's lectern block entity. */
public final class FotonLectern extends FotonTileState implements Lectern {
    private final FotonLecternInventory inventory;

    FotonLectern(FotonBlock block, BlockData data) {
        super(block, data);
        this.inventory = new FotonLecternInventory(block);
    }

    @Override
    public Inventory getInventory() {
        return inventory;
    }
}
