import org.bukkit.Material;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.enchantment.PrepareItemEnchantEvent;
import org.bukkit.inventory.BlockInventoryHolder;
import org.bukkit.inventory.EnchantingInventory;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.view.EnchantmentView;
import org.bukkit.plugin.java.JavaPlugin;

/** Compiled against Paper, then loaded unchanged by Foton's plugin host. */
public final class PaperEnchantFixture extends JavaPlugin implements Listener {
    public static int calls;
    public static int completed;
    @Override public void onEnable() { getServer().getPluginManager().registerEvents(this, this); }

    @EventHandler
    public void prepare(PrepareItemEnchantEvent event) {
        EnchantmentView view = event.getView();
        InventoryView generic = view;
        check(generic.getType() == org.bukkit.event.inventory.InventoryType.ENCHANTING, "view type");
        check(generic.countSlots() == 38, "two table slots plus 36 visible player slots");
        check(generic == event.getEnchanter().getOpenInventory(), "callback live view identity");
        check(generic.getBottomInventory().getSize() == 41, "real player inventory shape");
        check(generic.getPlayer().getUniqueId().equals(event.getEnchanter().getUniqueId()), "player");
        check(event.getEnchantBlock().getX() == 8 && event.getEnchantBlock().getY() == 64, "table position");
        check(event.getEnchantmentBonus() == 7, "bookshelf bonus");
        EnchantingInventory inventory = view.getTopInventory();
        check(inventory == event.getInventory(), "event inventory");
        check(((BlockInventoryHolder) inventory.getHolder()).getBlock() == event.getEnchantBlock(), "block holder");
        check(inventory.getLocation().getBlockX() == 8, "inventory location");
        check(inventory.getItem() == event.getItem(), "item mirror during callback");
        if (calls++ == 0) {
            check(!event.isCancelled(), "enchantable input starts allowed");
            event.setCancelled(true);
            completed++;
            return;
        }
        check(event.isCancelled(), "non-enchantable input starts cancelled");
        event.setCancelled(false);
        event.getOffers()[0] = new EnchantmentOffer(Enchantment.UNBREAKING, 3, 9);
        event.getOffers()[1] = null;
        check(view.getOffers()[0].getCost() == 2, "event offers do not mutate view until applied");
        EnchantmentOffer[] copy = view.getOffers();
        copy[0].setCost(31);
        check(view.getOffers()[0].getCost() == 2, "view reads return independent offers");
        view.setEnchantmentSeed(Integer.MIN_VALUE + 7);
        check(view.getEnchantmentSeed() == Integer.MIN_VALUE + 7, "full-width seed");
        event.getItem().setType(Material.DIAMOND_SWORD);
        inventory.setSecondary(new ItemStack(Material.LAPIS_LAZULI, 3));
        check(generic.getItem(1).getAmount() == 3, "raw-slot inventory write");
        completed++;
    }

    private static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
