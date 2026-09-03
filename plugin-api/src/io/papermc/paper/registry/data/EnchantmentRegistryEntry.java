package io.papermc.paper.registry.data;

import io.papermc.paper.registry.RegistryBuilder;
import io.papermc.paper.registry.set.RegistryKeySet;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import net.kyori.adventure.text.Component;
import org.bukkit.enchantments.Enchantment;
import org.bukkit.inventory.EquipmentSlotGroup;

/** Data used to create a registry enchantment. */
public interface EnchantmentRegistryEntry {
    Component description();
    RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems();
    RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems();
    int weight();
    int maxLevel();
    EnchantmentCost minimumCost();
    EnchantmentCost maximumCost();
    int anvilCost();
    List<EquipmentSlotGroup> activeSlots();
    RegistryKeySet<Enchantment> exclusiveWith();

    interface Builder extends EnchantmentRegistryEntry, RegistryBuilder<Enchantment> {
        Builder description(Component description);
        Builder supportedItems(RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems);
        Builder primaryItems(RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems);
        Builder weight(int weight);
        Builder maxLevel(int maxLevel);
        Builder minimumCost(EnchantmentCost minimumCost);
        Builder maximumCost(EnchantmentCost maximumCost);
        Builder anvilCost(int anvilCost);

        default Builder activeSlots(EquipmentSlotGroup... activeSlots) {
            List<EquipmentSlotGroup> values = new ArrayList<>(activeSlots.length);
            Collections.addAll(values, activeSlots);
            return activeSlots(values);
        }

        Builder activeSlots(Iterable<EquipmentSlotGroup> activeSlots);
        Builder exclusiveWith(RegistryKeySet<Enchantment> exclusiveWith);
    }

    interface EnchantmentCost {
        int baseCost();
        int additionalPerLevelCost();

        static EnchantmentCost of(int baseCost, int additionalPerLevelCost) {
            return new EnchantmentCostImpl(baseCost, additionalPerLevelCost);
        }
    }
}

final class EnchantmentCostImpl implements EnchantmentRegistryEntry.EnchantmentCost {
    private final int baseCost;
    private final int additionalPerLevelCost;

    EnchantmentCostImpl(int baseCost, int additionalPerLevelCost) {
        this.baseCost = baseCost;
        this.additionalPerLevelCost = additionalPerLevelCost;
    }

    @Override public int baseCost() { return baseCost; }
    @Override public int additionalPerLevelCost() { return additionalPerLevelCost; }

    @Override
    public boolean equals(Object other) {
        return other instanceof EnchantmentRegistryEntry.EnchantmentCost cost
            && baseCost == cost.baseCost()
            && additionalPerLevelCost == cost.additionalPerLevelCost();
    }

    @Override public int hashCode() { return 31 * baseCost + additionalPerLevelCost; }
    @Override public String toString() { return "EnchantmentCost[baseCost=" + baseCost + ", additionalPerLevelCost=" + additionalPerLevelCost + "]"; }
}

/** Mutable builder implementation used by the Steel registry bridge. */
final class EnchantmentBuilderImpl implements EnchantmentRegistryEntry.Builder {
    private Component description;
    private RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems;
    private RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems;
    private int weight;
    private int maxLevel;
    private EnchantmentRegistryEntry.EnchantmentCost minimumCost;
    private EnchantmentRegistryEntry.EnchantmentCost maximumCost;
    private int anvilCost;
    private List<EquipmentSlotGroup> activeSlots = List.of();
    private RegistryKeySet<Enchantment> exclusiveWith;

    @Override public Component description() { return description; }
    @Override public RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems() { return supportedItems; }
    @Override public RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems() { return primaryItems; }
    @Override public int weight() { return weight; }
    @Override public int maxLevel() { return maxLevel; }
    @Override public EnchantmentRegistryEntry.EnchantmentCost minimumCost() { return minimumCost; }
    @Override public EnchantmentRegistryEntry.EnchantmentCost maximumCost() { return maximumCost; }
    @Override public int anvilCost() { return anvilCost; }
    @Override public List<EquipmentSlotGroup> activeSlots() { return activeSlots; }
    @Override public RegistryKeySet<Enchantment> exclusiveWith() { return exclusiveWith; }

    @Override public Builder description(Component value) { description = value; return this; }
    @Override public Builder supportedItems(RegistryKeySet<org.bukkit.inventory.ItemType> value) { supportedItems = value; return this; }
    @Override public Builder primaryItems(RegistryKeySet<org.bukkit.inventory.ItemType> value) { primaryItems = value; return this; }
    @Override public Builder weight(int value) { weight = value; return this; }
    @Override public Builder maxLevel(int value) { maxLevel = value; return this; }
    @Override public Builder minimumCost(EnchantmentRegistryEntry.EnchantmentCost value) { minimumCost = value; return this; }
    @Override public Builder maximumCost(EnchantmentRegistryEntry.EnchantmentCost value) { maximumCost = value; return this; }
    @Override public Builder anvilCost(int value) { anvilCost = value; return this; }

    @Override
    public Builder activeSlots(Iterable<EquipmentSlotGroup> values) {
        List<EquipmentSlotGroup> copy = new ArrayList<>();
        for (EquipmentSlotGroup value : values) copy.add(value);
        activeSlots = List.copyOf(copy);
        return this;
    }

    @Override public Builder exclusiveWith(RegistryKeySet<Enchantment> value) { exclusiveWith = value; return this; }

    EnchantmentRegistryEntry snapshot() {
        return new EnchantmentEntrySnapshot(description, supportedItems, primaryItems, weight, maxLevel,
            minimumCost, maximumCost, anvilCost, activeSlots, exclusiveWith);
    }
}

final class EnchantmentEntrySnapshot implements EnchantmentRegistryEntry {
    private final Component description;
    private final RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems;
    private final RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems;
    private final int weight;
    private final int maxLevel;
    private final EnchantmentCost minimumCost;
    private final EnchantmentCost maximumCost;
    private final int anvilCost;
    private final List<EquipmentSlotGroup> activeSlots;
    private final RegistryKeySet<Enchantment> exclusiveWith;

    EnchantmentEntrySnapshot(Component description, RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems,
            RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems, int weight, int maxLevel,
            EnchantmentCost minimumCost, EnchantmentCost maximumCost, int anvilCost,
            List<EquipmentSlotGroup> activeSlots, RegistryKeySet<Enchantment> exclusiveWith) {
        this.description = description;
        this.supportedItems = supportedItems;
        this.primaryItems = primaryItems;
        this.weight = weight;
        this.maxLevel = maxLevel;
        this.minimumCost = minimumCost;
        this.maximumCost = maximumCost;
        this.anvilCost = anvilCost;
        this.activeSlots = List.copyOf(activeSlots);
        this.exclusiveWith = exclusiveWith;
    }

    @Override public Component description() { return description; }
    @Override public RegistryKeySet<org.bukkit.inventory.ItemType> supportedItems() { return supportedItems; }
    @Override public RegistryKeySet<org.bukkit.inventory.ItemType> primaryItems() { return primaryItems; }
    @Override public int weight() { return weight; }
    @Override public int maxLevel() { return maxLevel; }
    @Override public EnchantmentCost minimumCost() { return minimumCost; }
    @Override public EnchantmentCost maximumCost() { return maximumCost; }
    @Override public int anvilCost() { return anvilCost; }
    @Override public List<EquipmentSlotGroup> activeSlots() { return activeSlots; }
    @Override public RegistryKeySet<Enchantment> exclusiveWith() { return exclusiveWith; }
}
