import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.entity.HumanEntity;
import org.bukkit.event.enchantment.PrepareItemEnchantEvent;
import org.bukkit.inventory.*;
import org.bukkit.inventory.view.EnchantmentView;

/** Exercises both constructor descriptors and the covariant/erased accessor bridge. */
final class PrepareEnchantBridgeCheck {
    static class GenericView implements InventoryView {
        public Inventory getTopInventory() { return null; }
        public Inventory getBottomInventory() { return null; }
        public HumanEntity getPlayer() { return null; }
        public String getTitle() { return "bridge"; }
    }
    static final class TypedView extends GenericView implements EnchantmentView {
        public EnchantingInventory getTopInventory() { return null; }
        public int getEnchantmentSeed() { return 12; }
        public void setEnchantmentSeed(int seed) {}
        public EnchantmentOffer[] getOffers() { return new EnchantmentOffer[3]; }
        public void setOffers(EnchantmentOffer[] offers) {}
    }
    public static void main(String[] args) { run(); }
    static void run() {
        EnchantmentView view = new TypedView();
        var offers = new EnchantmentOffer[] {null, new EnchantmentOffer(null, 2, 7), null};
        var broad = new PrepareItemEnchantEvent(null, (InventoryView) view, null, null, offers, 4);
        var typed = new PrepareItemEnchantEvent(null, view, null, null, offers, 4);
        if (broad.getView() != view || typed.getView() != view
                || ((org.bukkit.event.inventory.InventoryEvent) typed).getView() != view)
            throw new AssertionError("typed view identity through both constructor/accessor descriptors");
        if (typed.getOffers() != offers || typed.getExpLevelCostsOffered()[1] != 7)
            throw new AssertionError("live offers or null-slot cost conversion lost");
        int[] costs = typed.getExpLevelCostsOffered();
        costs[1] = 99;
        if (typed.getExpLevelCostsOffered()[1] != 7)
            throw new AssertionError("cost snapshot aliases offers");
        if (new PrepareItemEnchantEvent(null, (InventoryView) null, null, null, offers, 0).getView() != null)
            throw new AssertionError("legacy null view rejected");
        try {
            new PrepareItemEnchantEvent(null, new GenericView(), null, null, offers, 0);
            throw new AssertionError("incompatible generic view accepted at construction");
        } catch (IllegalArgumentException expected) {
            if (!expected.getMessage().contains("EnchantmentView"))
                throw new AssertionError("view boundary lacks actionable diagnostic", expected);
        }
    }
}
