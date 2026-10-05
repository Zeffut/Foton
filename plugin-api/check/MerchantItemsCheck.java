import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.MerchantRecipe;

final class MerchantItemsCheck {
    static void check() {
        MerchantRecipe recipe = new MerchantRecipe(new ItemStack(Material.DIAMOND), 8);
        try { new foton.item.MerchantOfferMutation(recipe); throw new AssertionError("zero ingredients accepted"); }
        catch (IllegalStateException expected) { }
        ItemStack zero = new ItemStack(Material.STONE);
        zero.setAmount(0);
        for (ItemStack invalid : new ItemStack[]{zero, new ItemStack(Material.AIR)}) {
            try { recipe.addIngredient(invalid); throw new AssertionError("empty ingredient accepted"); }
            catch (IllegalArgumentException expected) { }
            Checks.expect(recipe.getIngredients().isEmpty(), "failed empty ingredient changed recipe");
        }
        recipe.addIngredient(new ItemStack(Material.EMERALD));
        recipe.addIngredient(new ItemStack(Material.STONE));
        try { recipe.addIngredient(new ItemStack(Material.DIRT)); throw new AssertionError("third ingredient accepted"); }
        catch (IllegalStateException expected) { }
        Checks.expect(recipe.getIngredients().size() == 2, "failed third ingredient changed recipe");
        try { recipe.encode(); throw new AssertionError("lossy merchant encoding accepted"); }
        catch (UnsupportedOperationException expected) { }
        Checks.expect(MerchantRecipe.decode("minecraft:diamond 1|0|8|0|minecraft:emerald 2|") != null, "represented legacy offer");
        Checks.expect(MerchantRecipe.decode("minecraft:diamond 1|0|8|0|minecraft:emerald 2|broken") == null, "malformed second cost must reject whole legacy record");
        Checks.expect(MerchantRecipe.decode("minecraft:diamond 1|0|8|0|minecraft:emerald 0|") == null, "zero legacy cost must reject whole record");
        for (int count : new int[]{0, -1})
            Checks.expect(MerchantRecipe.decode("minecraft:diamond " + count + "|0|8|0|minecraft:emerald 2|") == null, "invalid result count must reject whole legacy record");
        for (ItemStack empty : new ItemStack[]{null, zero, new ItemStack(Material.AIR)}) {
            MerchantRecipe invalid = new MerchantRecipe(empty, 8);
            invalid.addIngredient(new ItemStack(Material.EMERALD));
            try { new foton.item.MerchantOfferMutation(invalid); throw new AssertionError("empty merchant result accepted"); }
            catch (IllegalArgumentException expected) { }
        }
    }
}
