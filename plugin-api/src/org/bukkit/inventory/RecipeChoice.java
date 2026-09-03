package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.Material;

/** An ingredient predicate used by Bukkit crafting recipes. */
public interface RecipeChoice {
    boolean test(ItemStack stack);
    ItemStack getItemStack();

    /** Matches any one of a set of materials. */
    class MaterialChoice implements RecipeChoice {
        private final List<Material> choices;
        public MaterialChoice(Material... choices) {
            this.choices = new ArrayList<>();
            if (choices != null) for (Material choice : choices) if (choice != null) this.choices.add(choice);
        }
        public MaterialChoice(org.bukkit.Tag<Material> tag) {
            this(tag == null ? java.util.Collections.emptyList() : tag.getValues().stream().toList());
        }
        public MaterialChoice(List<Material> choices) {
            this(choices == null ? new Material[0] : choices.toArray(new Material[0]));
        }
        public List<Material> getChoices() { return Collections.unmodifiableList(choices); }
        @Override public boolean test(ItemStack stack) {
            return stack != null && choices.contains(stack.getType());
        }
        @Override public ItemStack getItemStack() {
            return new ItemStack(choices.isEmpty() ? Material.AIR : choices.get(0));
        }
    }

    /** Matches an item type and its metadata. */
    class ExactChoice implements RecipeChoice {
        private final ItemStack stack;
        public ExactChoice(ItemStack stack) { this.stack = stack == null ? new ItemStack(Material.AIR) : stack.clone(); }
        @Override public boolean test(ItemStack candidate) { return stack.isSimilar(candidate); }
        public List<ItemStack> getChoices() { return Collections.singletonList(getItemStack()); }
        @Override public ItemStack getItemStack() { return stack.clone(); }
    }
}
