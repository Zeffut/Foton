package org.bukkit.inventory;

import org.bukkit.Material;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** A mutable merchant offer, mirroring the offer counters used by villagers. */
public class MerchantRecipe {
    private ItemStack result;
    private int uses;
    private int maxUses;
    private int demand;
    private boolean experienceReward;
    private int villagerExperience;
    private float priceMultiplier;
    private int specialPrice;
    private java.util.function.Consumer<MerchantRecipe> writeBack;
    private String owner;
    private int offerIndex = -1;
    private List<ItemStack> ingredients = new ArrayList<>();

    public MerchantRecipe(ItemStack result, int uses, int maxUses, boolean experienceReward,
            int villagerExperience, float priceMultiplier, int demand) {
        this.result = result == null ? null : result.clone();
        this.uses = Math.max(0, uses);
        this.maxUses = Math.max(1, maxUses);
        this.experienceReward = experienceReward;
        this.villagerExperience = Math.max(0, villagerExperience);
        this.priceMultiplier = priceMultiplier;
        this.demand = demand;
    }

    public MerchantRecipe(ItemStack result, int maxUses, int villagerExperience,
            boolean experienceReward) {
        this(result, 0, maxUses, experienceReward, villagerExperience, 0.05f, 0);
    }

    public MerchantRecipe(ItemStack result, int maxUses) {
        this(result, 0, maxUses, true, 0, 0.05f, 0);
    }

    public MerchantRecipe(ItemStack result, int uses, int maxUses, boolean experienceReward,
            int villagerExperience, float priceMultiplier) {
        this(result, uses, maxUses, experienceReward, villagerExperience, priceMultiplier, 0);
    }

    public static MerchantRecipe decode(String encoded) { return decode(encoded, null, -1); }

    public static MerchantRecipe decode(String encoded, String owner, int offerIndex) {
        if (encoded == null) return null;
        String[] fields = encoded.split("\\|", -1);
        if (fields.length != 4 && fields.length != 6) return null;
        String[] item = fields[0].trim().split(" ", -1);
        if (item.length != 2) return null;
        Material material = Material.matchMaterial(item[0]);
        if (material == null) return null;
        try {
            ItemStack result = new ItemStack(material, Integer.parseInt(item[1]));
            MerchantRecipe recipe = new MerchantRecipe(result, Integer.parseInt(fields[1]),
                Integer.parseInt(fields[2]), true, 0, 0.05f,
                Integer.parseInt(fields[3]));
            recipe.owner = owner;
            recipe.offerIndex = offerIndex;
            if (fields.length == 6) {
                recipe.ingredients.add(parseItem(fields[4]));
                ItemStack second = parseItem(fields[5]);
                if (second != null && !second.getType().isAir()) recipe.ingredients.add(second);
                recipe.ingredients.removeIf(java.util.Objects::isNull);
            }
            return recipe;
        } catch (NumberFormatException error) {
            return null;
        }
    }

    /** Applies this offer's vanilla demand surcharge in place. */
    public void adjust(ItemStack ingredient) {
        if (ingredient == null) return;
        int demandDiff = (int) Math.floor(ingredient.getAmount() * Math.max(0, demand) * priceMultiplier);
        int count = Math.max(1, Math.min(ingredient.getMaxStackSize(), ingredient.getAmount() + demandDiff));
        ingredient.setAmount(count);
    }

    public ItemStack getResult() { return result == null ? null : result.clone(); }
    public void setResult(ItemStack value) { result = value == null ? null : value.clone(); }
    public int getUses() { return uses; }
    public void setUses(int value) { uses = Math.max(0, value); if (owner != null) foton.Native.entitySetMerchantOfferUses(owner, offerIndex, uses); changed(); }
    public int getMaxUses() { return maxUses; }
    public void setMaxUses(int value) { maxUses = Math.max(1, value); if (owner != null) foton.Native.entitySetMerchantOfferMaxUses(owner, offerIndex, maxUses); changed(); }
    public int getDemand() { return demand; }
    public void setDemand(int value) { demand = value; if (owner != null) foton.Native.entitySetMerchantOfferDemand(owner, offerIndex, demand); changed(); }
    public boolean hasExperienceReward() { return experienceReward; }
    public void setExperienceReward(boolean flag) { experienceReward = flag; changed(); }
    public int getVillagerExperience() { return villagerExperience; }
    public void setVillagerExperience(int villagerExperience) { this.villagerExperience = villagerExperience; changed(); }
    public float getPriceMultiplier() { return priceMultiplier; }
    public void setPriceMultiplier(float priceMultiplier) { this.priceMultiplier = priceMultiplier; changed(); }
    public int getSpecialPrice() { return specialPrice; }
    public void setSpecialPrice(int specialPrice) { this.specialPrice = specialPrice; changed(); }

    /** Makes this recipe live: each field set afterwards is handed to {@code sink},
     * the way Paper's recipes read from a merchant write through to its offer. */
    public void writeBackTo(java.util.function.Consumer<MerchantRecipe> sink) { writeBack = sink; }
    private void changed() { if (writeBack != null) writeBack.accept(this); }

    private static final String FIELD = "\u001e";

    /** Every field of the offer, items with their components, for a plugin merchant. */
    public String encodeOffer() {
        ItemStack first = ingredients.size() > 0 ? ingredients.get(0) : null;
        ItemStack second = ingredients.size() > 1 ? ingredients.get(1) : null;
        return String.join(FIELD, foton.FotonInventory.encode(result), Integer.toString(uses),
            Integer.toString(maxUses), experienceReward ? "1" : "0", Integer.toString(villagerExperience),
            Float.toString(priceMultiplier), Integer.toString(demand), Integer.toString(specialPrice),
            foton.FotonInventory.encode(first), foton.FotonInventory.encode(second));
    }

    /** Reads what {@link #encodeOffer} writes; null if it is malformed. */
    public static MerchantRecipe decodeOffer(String encoded) {
        if (encoded == null) return null;
        String[] fields = encoded.split(FIELD, -1);
        if (fields.length != 10) return null;
        ItemStack result = foton.FotonInventory.decode(fields[0]);
        if (result == null) return null;
        try {
            MerchantRecipe recipe = new MerchantRecipe(result, Integer.parseInt(fields[1]), Integer.parseInt(fields[2]),
                "1".equals(fields[3]), Integer.parseInt(fields[4]), Float.parseFloat(fields[5]), Integer.parseInt(fields[6]));
            recipe.specialPrice = Integer.parseInt(fields[7]);
            recipe.addIngredient(foton.FotonInventory.decode(fields[8]));
            recipe.addIngredient(foton.FotonInventory.decode(fields[9]));
            return recipe;
        } catch (NumberFormatException error) {
            return null;
        }
    }
    /** Encodes the Vanilla fields understood by Foton's merchant bridge. */
    public String encode() {
        ItemStack first = ingredients.size() > 0 ? ingredients.get(0) : null;
        ItemStack second = ingredients.size() > 1 ? ingredients.get(1) : null;
        return item(result) + "|" + uses + "|" + maxUses + "|" + demand + "|" + item(first) + "|" + item(second);
    }
    private static String item(ItemStack value) { return value == null || value.getType().isAir() ? "" : value.getType().getKey() + " " + value.getAmount(); }
    public List<ItemStack> getIngredients() {
        ArrayList<ItemStack> copy = new ArrayList<>(ingredients.size());
        for (ItemStack item : ingredients) copy.add(item.clone());
        return Collections.unmodifiableList(copy);
    }

    public void addIngredient(ItemStack ingredient) {
        if (ingredient != null) ingredients.add(ingredient.clone());
    }
    public void setIngredients(List<ItemStack> values) {
        ingredients.clear();
        if (values != null) for (ItemStack value : values) addIngredient(value);
    }

    private static ItemStack parseItem(String encoded) {
        if (encoded == null || encoded.isEmpty()) return null;
        String[] fields = encoded.trim().split(" ", -1);
        if (fields.length != 2) return null;
        Material material = Material.matchMaterial(fields[0]);
        if (material == null) return null;
        try { return new ItemStack(material, Integer.parseInt(fields[1])); }
        catch (NumberFormatException ignored) { return null; }
    }
}
