package foton;

import org.bukkit.entity.HumanEntity;
import org.bukkit.entity.Player;
import org.bukkit.inventory.Inventory;
import org.bukkit.inventory.InventoryView;

/** Live view facade for the menu currently installed on one player. */
public final class FotonInventoryView extends InventoryView {
    private final FotonPlayer player;
    private final Inventory top;
    private String title;

    public FotonInventoryView(FotonPlayer player) { this(player, null); }

    FotonInventoryView(FotonPlayer player, Inventory suppliedTop) {
        this.player = player;
        this.top = suppliedTop != null ? suppliedTop : liveTop(player.getUniqueId().toString());
    }

    private static Inventory liveTop(String owner) {
        String menuType = Native.openMenuType(owner);
        if ("minecraft:crafting".equals(menuType)) return new FotonCraftingInventory(owner);
        if ("minecraft:grindstone".equals(menuType)) return new FotonGrindstoneInventory(owner);
        return new FotonMenuInventory(owner);
    }

    @Override public Inventory getTopInventory() { return top; }
    @Override public Inventory getBottomInventory() { return player.getInventory(); }
    @Override public HumanEntity getPlayer() { return player; }
    /** Read on first use, so an event view built from a snapshot needs no live menu. */
    @Override public String getTitle() {
        if (title == null) {
            String live = Native.openMenuTitle(player.getUniqueId().toString());
            title = live == null ? "" : live;
        }
        return title;
    }
}
