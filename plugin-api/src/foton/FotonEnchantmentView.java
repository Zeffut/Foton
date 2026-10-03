package foton;

import org.bukkit.NamespacedKey;
import org.bukkit.block.Block;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.enchantments.EnchantmentOffer;
import org.bukkit.inventory.EnchantingInventory;
import org.bukkit.inventory.view.EnchantmentView;

/** Full-width menu properties, independent of Minecraft's short data packets. */
final class FotonEnchantmentView extends FotonInventoryView implements EnchantmentView {
    private final String owner;
    private final long instance;
    private final FotonEnchantingInventory inventory;
    private State preparing;

    FotonEnchantmentView(FotonPlayer player, long instance, Block table, org.bukkit.inventory.ItemStack[] items, State state) {
        this(player, instance, new FotonEnchantingInventory(player, instance, table, items), state);
    }

    private FotonEnchantmentView(FotonPlayer player, long instance, FotonEnchantingInventory inventory, State state) {
        super(player, inventory);
        this.owner = player.getUniqueId().toString();
        this.instance = instance;
        this.inventory = inventory;
        this.preparing = state;
    }

    void finishPrepare() { preparing = null; inventory.finishPrepare(); }
    @Override public EnchantingInventory getTopInventory() { return inventory; }
    @Override public int countSlots() { return 38; }
    @Override public String getTitle() {
        String title = preparing != null ? Native.openMenuTitle(owner) : Native.enchantmentTitle(owner, instance);
        if (title == null) throw new IllegalStateException("The enchanting view is no longer open");
        return title;
    }
    @Override public void close() {
        if (preparing != null) super.close();
        else if (!Native.closeEnchantmentView(owner, instance)) throw new IllegalStateException("The enchanting view is no longer open");
    }
    @Override public int getEnchantmentSeed() { return state().seed; }
    @Override public void setEnchantmentSeed(int seed) {
        State state = state();
        state.seed = seed;
        update(state);
    }
    @Override public EnchantmentOffer[] getOffers() { return state().copyOffers(); }
    @Override public void setOffers(EnchantmentOffer[] offers) {
        if (offers == null || offers.length != 3) throw new IllegalArgumentException("There must be 3 offers given");
        State state = state();
        for (int index = 0; index < 3; index++) {
            state.offers[index] = State.copy(offers[index]);
            state.costs[index] = offers[index] == null ? 0 : offers[index].getCost();
        }
        update(state);
    }

    private State state() {
        if (preparing != null) return preparing;
        String value = Native.enchantmentView(owner);
        if (value == null || Long.parseUnsignedLong(value.substring(0, value.indexOf(' '))) != instance)
            throw new IllegalStateException("The enchanting view is no longer open");
        return State.decode(value.substring(value.indexOf('\u001f') + 1));
    }

    private void update(State state) {
        if (preparing == null && !Native.setEnchantmentView(owner, instance, state.encode()))
            throw new IllegalStateException("The enchanting view is no longer open");
    }

    static final class State {
        int seed;
        final int[] costs = new int[3];
        final EnchantmentOffer[] offers = new EnchantmentOffer[3];

        static State decode(String encoded) {
            String[] parts = encoded.split(";", -1);
            if (parts.length != 4) throw new IllegalArgumentException("Malformed enchanting view");
            State state = new State();
            state.seed = Integer.parseInt(parts[0]);
            for (int index = 0; index < 3; index++) {
                String[] fields = parts[index + 1].split(",", -1);
                if (fields.length != 3) throw new IllegalArgumentException("Malformed enchanting offer");
                state.costs[index] = Integer.parseInt(fields[0]);
                if (!fields[1].isEmpty()) {
                    Enchantment enchantment = Enchantment.getByKey(NamespacedKey.fromString(fields[1]));
                    if (enchantment == null) throw new IllegalArgumentException("Unknown enchantment: " + fields[1]);
                    state.offers[index] = new EnchantmentOffer(enchantment, Integer.parseInt(fields[2]), state.costs[index]);
                }
            }
            return state;
        }

        String encode() {
            StringBuilder out = new StringBuilder().append(seed);
            for (int index = 0; index < 3; index++) {
                EnchantmentOffer offer = offers[index];
                out.append(';').append(costs[index]).append(',');
                if (offer != null) out.append(offer.getEnchantment().getKey());
                out.append(',').append(offer == null ? -1 : offer.getEnchantmentLevel());
            }
            return out.toString();
        }

        EnchantmentOffer[] copyOffers() {
            EnchantmentOffer[] result = new EnchantmentOffer[3];
            for (int index = 0; index < 3; index++) result[index] = copy(offers[index]);
            return result;
        }

        static EnchantmentOffer copy(EnchantmentOffer offer) {
            return offer == null ? null : new EnchantmentOffer(offer.getEnchantment(), offer.getEnchantmentLevel(), offer.getCost());
        }

        void applyEventOffers(EnchantmentOffer[] values) {
            for (int index = 0; index < 3; index++) {
                if (values[index] != null) costs[index] = values[index].getCost();
                else if (offers[index] != null) costs[index] = 0;
                offers[index] = copy(values[index]);
            }
        }
    }
}
