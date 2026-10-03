package foton;

import org.bukkit.block.Block;
import org.bukkit.event.enchantment.PrepareItemEnchantEvent;
import org.bukkit.inventory.InventoryView;
import org.bukkit.inventory.ItemStack;

/** One synchronous prepare call. The player's containers are unlocked across JNI. */
public final class EnchantmentEventBridge {
    private EnchantmentEventBridge() { }
    private static final ThreadLocal<FotonEnchantmentView> PREPARING = new ThreadLocal<>();

    static InventoryView currentView(FotonPlayer player) {
        FotonEnchantmentView preparing = PREPARING.get();
        if (preparing != null && preparing.getPlayer().getUniqueId().equals(player.getUniqueId())) return preparing;
        if (FotonCustomInventory.openForViewer(player.getUniqueId().toString()) != null)
            return new FotonInventoryView(player);
        String encoded = Native.enchantmentView(player.getUniqueId().toString());
        if (encoded == null) return new FotonInventoryView(player);
        String[] table = encoded.substring(0, encoded.indexOf('\u001f')).split(" ");
        long instance = Long.parseUnsignedLong(table[0]);
        Block block = new FotonBlock(new FotonWorld(table[1]), Integer.parseInt(table[2]), Integer.parseInt(table[3]), Integer.parseInt(table[4]));
        return new FotonEnchantmentView(player, instance, block, null, null);
    }

    public static String prepare(String uuid, String world, int x, int y, int z,
            String input, String lapis, int bonus, String encodedState, boolean cancelled) {
        FotonPlayer player = new FotonPlayer(java.util.UUID.fromString(uuid));
        Block block = new FotonBlock(new FotonWorld(world), x, y, z);
        ItemStack[] items = { FotonInventory.decode(input), FotonInventory.decode(lapis) };
        int separator = encodedState.indexOf('|');
        long instance = Long.parseUnsignedLong(encodedState.substring(0, separator));
        FotonEnchantmentView.State state = FotonEnchantmentView.State.decode(encodedState.substring(separator + 1));
        FotonEnchantmentView view = new FotonEnchantmentView(player, instance, block, items, state);
        PrepareItemEnchantEvent event = new PrepareItemEnchantEvent(player, view, block, items[0], state.copyOffers(), bonus);
        event.setCancelled(cancelled);
        FotonEnchantmentView previous = PREPARING.get();
        PREPARING.set(view);
        try {
            EventBridge.dispatch(event);
            state.applyEventOffers(event.getOffers());
            return (event.isCancelled() ? "1" : "0") + '\u001f' + state.encode()
                + '\u001f' + FotonInventory.encode(items[0]) + '\u001f' + FotonInventory.encode(items[1]);
        } finally {
            if (previous == null) PREPARING.remove(); else PREPARING.set(previous);
            view.finishPrepare();
        }
    }
}
