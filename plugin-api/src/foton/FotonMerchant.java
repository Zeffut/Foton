package foton;

import java.lang.ref.Cleaner;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.entity.HumanEntity;
import org.bukkit.inventory.Merchant;
import org.bukkit.inventory.MerchantRecipe;

/** A merchant made by {@code Bukkit.createMerchant}: offers and a trading
 * screen with no villager behind them. The offers live on the server, so a
 * trade's uses show up in what this answers next. */
public final class FotonMerchant implements Merchant {
    private static final Cleaner CLEANER = Cleaner.create();
    private final String handle;

    public FotonMerchant(net.kyori.adventure.text.Component title) {
        String created = Native.createMerchant(title == null ? null : FotonComponents.toJson(title));
        if (created == null) throw new IllegalStateException("The server could not create a merchant");
        handle = created;
        CLEANER.register(this, () -> Native.releaseMerchant(created));
    }

    public String handle() { return handle; }

    @Override public List<MerchantRecipe> getRecipes() {
        String[] encoded = Native.merchantOffers(handle);
        if (encoded == null) return Collections.emptyList();
        ArrayList<MerchantRecipe> recipes = new ArrayList<>(encoded.length);
        for (int index = 0; index < encoded.length; index++) {
            MerchantRecipe recipe = MerchantRecipe.decodeOffer(encoded[index]);
            if (recipe == null) continue;
            int offer = index;
            recipe.writeBackTo(changed -> Native.setMerchantOffer(handle, offer, changed.encodeOffer()));
            recipes.add(recipe);
        }
        return Collections.unmodifiableList(recipes);
    }

    @Override public void setRecipes(List<MerchantRecipe> recipes) {
        List<MerchantRecipe> values = recipes == null ? List.of() : recipes;
        String[] encoded = new String[values.size()];
        for (int index = 0; index < encoded.length; index++) {
            MerchantRecipe recipe = values.get(index);
            if (recipe == null) throw new IllegalArgumentException("recipe " + index + " is null");
            encoded[index] = recipe.encodeOffer();
        }
        if (!Native.setMerchantOffers(handle, encoded))
            throw new IllegalArgumentException("Each recipe needs a result and at least one ingredient");
    }

    @Override public HumanEntity getTrader() {
        String trader = Native.merchantTrader(handle);
        return trader == null ? null : org.bukkit.Bukkit.getPlayer(java.util.UUID.fromString(trader));
    }

    @Override public String toString() { return "FotonMerchant{" + handle + "}"; }
}
