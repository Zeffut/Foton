package foton.item;

/** Owning ingredient/result inputs and all native offer scalars. */
public final class MerchantOfferMutation {
    private final ItemMutation result;
    private final ItemMutation[] ingredients;
    private final int uses, maxUses, demand, experience, specialPrice;
    private final float priceMultiplier;
    private final boolean rewardExperience;

    public MerchantOfferMutation(org.bukkit.inventory.MerchantRecipe recipe) {
        java.util.Objects.requireNonNull(recipe, "merchant recipe");
        java.util.List<org.bukkit.inventory.ItemStack> costs = recipe.getIngredients();
        if (costs.isEmpty()) throw new IllegalStateException("merchant recipe has no ingredients");
        if (costs.size() > 2) throw new IllegalStateException("merchant recipe has more than two ingredients");
        ingredients = new ItemMutation[costs.size()];
        for (int index = 0; index < costs.size(); index++) {
            org.bukkit.inventory.ItemStack cost = costs.get(index);
            if (cost == null || cost.getType().isAir() || cost.getAmount() <= 0)
                throw new IllegalArgumentException("empty merchant ingredient");
            ingredients[index] = cost.nativeMutation();
        }
        org.bukkit.inventory.ItemStack output = recipe.getResult();
        result = output == null ? ItemMutation.empty() : output.nativeMutation();
        uses = recipe.getUses(); maxUses = recipe.getMaxUses(); demand = recipe.getDemand();
        experience = recipe.getVillagerExperience(); priceMultiplier = recipe.getPriceMultiplier();
        rewardExperience = recipe.hasExperienceReward(); specialPrice = recipe.getSpecialPrice();
    }
}
