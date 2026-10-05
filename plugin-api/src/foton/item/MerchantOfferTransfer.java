package foton.item;

/** Native result owners remain live until the Bukkit recipe has adopted them. */
public final class MerchantOfferTransfer {
    private final ItemTransfer result, first, second;
    private final int uses, maxUses, demand, experience, specialPrice;
    private final float priceMultiplier;
    private final boolean rewardExperience;

    private MerchantOfferTransfer(ItemTransfer result, ItemTransfer first, ItemTransfer second,
            int uses, int maxUses, int demand, int experience, int specialPrice,
            float priceMultiplier, boolean rewardExperience) {
        this.result = result; this.first = first; this.second = second;
        this.uses = uses; this.maxUses = maxUses; this.demand = demand;
        this.experience = experience; this.specialPrice = specialPrice;
        this.priceMultiplier = priceMultiplier; this.rewardExperience = rewardExperience;
    }

    public org.bukkit.inventory.MerchantRecipe recipe(String owner, int index) {
        org.bukkit.inventory.MerchantRecipe recipe = new org.bukkit.inventory.MerchantRecipe(
            foton.FotonInventory.decodeTransfer(result), uses, maxUses, rewardExperience,
            experience, priceMultiplier, demand);
        recipe.addIngredient(foton.FotonInventory.decodeTransfer(first));
        if (second != null) recipe.addIngredient(foton.FotonInventory.decodeTransfer(second));
        recipe.setSpecialPrice(specialPrice);
        recipe.hydrateNativeCounters(uses, maxUses, experience);
        recipe.attachNativeOffer(owner, index);
        return recipe;
    }
}
