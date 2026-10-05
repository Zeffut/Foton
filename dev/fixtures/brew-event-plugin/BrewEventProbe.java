package foton.fixture.brew;

import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.block.BrewingStand;
import org.bukkit.command.PluginCommand;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.inventory.BrewEvent;
import org.bukkit.inventory.BrewerInventory;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.java.JavaPlugin;

/** Runs unchanged on Paper and Foton; every check uses the public Paper API. */
public final class BrewEventProbe extends JavaPlugin implements Listener {
    private boolean allow;
    private boolean cancelled;
    private boolean accepted;
    private BrewerInventory inventory;
    private NamespacedKey marker;

    @Override public void onEnable() {
        marker = new NamespacedKey(this, "brew-result");
        getServer().getPluginManager().registerEvents(this, this);
        PluginCommand command = getCommand("brewprobe");
        if (command == null) throw new IllegalStateException("missing probe command");
        command.setExecutor((sender, ignored, label, args) -> {
            if (args.length != 1) return false;
            if (args[0].equals("allow")) {
                check(cancelled && inventory != null, "cancel callback was not completed");
                check(inventory.getItem(0).getType() == Material.POTION, "cancelled bottle changed");
                check(inventory.getIngredient().getType() == Material.NETHER_WART, "cancelled ingredient consumed");
                check(inventory.getFuel().getType() == Material.BLAZE_POWDER, "callback inventory edit lost");
                allow = true;
                System.out.println("[brew-probe] CANCELLED_INPUTS_RETAINED");
            } else if (args[0].equals("verify")) {
                check(accepted, "accepted callback was not completed");
                ItemStack result = inventory.getItem(0);
                check(result.getType() == Material.DIAMOND && result.getAmount() == 2, "result replacement lost");
                check(result.getItemMeta().getPersistentDataContainer().get(marker, PersistentDataType.INTEGER) == 42,
                    "result custom data lost");
                check(empty(inventory.getItem(1)) && empty(inventory.getItem(2)), "short result list did not clear bottles");
                check(empty(inventory.getIngredient()), "accepted ingredient was not consumed");
                System.out.println("[brew-probe] MODIFIED_RESULTS_PERSISTED");
            } else return false;
            return true;
        });
        System.out.println("[brew-probe] ENABLED");
    }

    @EventHandler public void onBrew(BrewEvent event) {
        if (event.getBlock().getX() != 2 || event.getBlock().getY() != 100 || event.getBlock().getZ() != 0) return;
        inventory = event.getContents();
        check(inventory.getSize() == 5, "brewer inventory size");
        check(inventory.getType() == org.bukkit.event.inventory.InventoryType.BREWING, "brewer inventory type");
        BrewingStand holder = inventory.getHolder();
        check(holder != null && holder.getX() == 2 && holder.getY() == 100, "real brewing stand holder");
        check(holder.getBrewingTime() == 0, "event must run at cycle completion");
        check(holder.getFuelLevel() == event.getFuelLevel(), "event fuel differs from native stand");
        check(inventory.getItem(0).getType() == Material.POTION, "callback input bottle");
        check(inventory.getIngredient().getType() == Material.NETHER_WART, "callback ingredient");
        // This JNI re-entry would deadlock if the tick still held its inventory lock.
        inventory.setFuel(new ItemStack(Material.BLAZE_POWDER));
        check(inventory.getFuel().getType() == Material.BLAZE_POWDER, "live inventory write");
        if (!allow) {
            event.setCancelled(true);
            cancelled = true;
            System.out.println("[brew-probe] CANCELLED_CALLBACK");
            return;
        }
        ItemStack result = new ItemStack(Material.DIAMOND, 2);
        ItemMeta meta = result.getItemMeta();
        meta.getPersistentDataContainer().set(marker, PersistentDataType.INTEGER, 42);
        result.setItemMeta(meta);
        event.getResults().clear();
        event.getResults().add(result);
        accepted = true;
        System.out.println("[brew-probe] MODIFIED_CALLBACK");
    }

    private static boolean empty(ItemStack item) { return item == null || item.getType().isAir(); }
    private static void check(boolean value, String message) {
        if (!value) throw new IllegalStateException(message);
    }
}
