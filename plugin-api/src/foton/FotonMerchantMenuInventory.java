package foton;

import org.bukkit.inventory.Merchant;
import org.bukkit.inventory.MerchantInventory;

/** The slots of a player's open trading screen, and who it trades with: the
 * villager or wandering trader it was opened on, or the merchant a plugin
 * opened. A plugin tells a vanilla villager's screen from its own by this. */
final class FotonMerchantMenuInventory extends FotonMenuInventory implements MerchantInventory {
    FotonMerchantMenuInventory(String owner) { super(owner); }

    @Override public Merchant getMerchant() {
        FotonMenuSource.Source source = FotonMenuSource.of(owner());
        if (source != null && source.entity() instanceof Merchant trader) return trader;
        return FotonMerchant.openedBy(owner());
    }
}
