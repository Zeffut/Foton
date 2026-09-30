package org.bukkit.enchantments;

/** One of the three offers an enchanting table shows. */
public class EnchantmentOffer {
    private Enchantment enchantment;
    private int enchantmentLevel;
    private int cost;

    public EnchantmentOffer(Enchantment enchantment, int enchantmentLevel, int cost) {
        this.enchantment = enchantment;
        this.enchantmentLevel = enchantmentLevel;
        this.cost = cost;
    }

    public Enchantment getEnchantment() { return enchantment; }
    public void setEnchantment(Enchantment enchantment) {
        if (enchantment == null) throw new IllegalArgumentException("The enchantment may not be null!");
        this.enchantment = enchantment;
    }
    public int getEnchantmentLevel() { return enchantmentLevel; }
    public void setEnchantmentLevel(int enchantmentLevel) {
        if (enchantmentLevel <= 0) throw new IllegalArgumentException("The enchantment level must be greater than 0!");
        this.enchantmentLevel = enchantmentLevel;
    }
    public int getCost() { return cost; }
    public void setCost(int cost) {
        if (cost <= 0) throw new IllegalArgumentException("The cost must be greater than 0!");
        this.cost = cost;
    }
}
