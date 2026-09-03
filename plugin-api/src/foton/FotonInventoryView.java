package foton;

import org.bukkit.entity.HumanEntity;
import org.bukkit.entity.Player;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryView;

/** Live view facade for the menu currently installed on one player. */
public final class FotonInventoryView extends InventoryView {
    private final FotonPlayer player;
    private final Inventory top;
    private final String title;

    public FotonInventoryView(FotonPlayer player) { this(player, null); }

    FotonInventoryView(FotonPlayer player, Inventory suppliedTop) {
        this.player = player;
        String owner = player.getUniqueId().toString();
        String menuType = Native.openMenuType(owner);
        this.top = suppliedTop != null ? suppliedTop : "minecraft:crafting".equals(menuType)
            ? new FotonCraftingInventory(owner)
            : "minecraft:grindstone".equals(menuType)
                ? new FotonGrindstoneInventory(owner)
                : new FotonMenuInventory(owner);
        String title = Native.openMenuTitle(player.getUniqueId().toString());
        this.title = title == null ? "" : title;
    }

    @Override public Inventory getTopInventory() { return top; }
    @Override public Inventory getBottomInventory() { return player.getInventory(); }
    @Override public HumanEntity getPlayer() { return player; }
    @Override public String getTitle() { return title; }
}
