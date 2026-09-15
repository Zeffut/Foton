package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.function.Predicate;
import org.bukkit.Material;

/** An ingredient predicate used by Bukkit crafting recipes. */
public interface RecipeChoice extends Predicate<ItemStack>, Cloneable {
    boolean test(ItemStack stack);
    ItemStack getItemStack();
    RecipeChoice clone();
    default RecipeChoice validate(boolean allowEmpty) { return this; }

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
        @Override public MaterialChoice clone() { return new MaterialChoice(choices); }
        @Override public MaterialChoice validate(boolean allowEmpty) {
            if (choices.stream().anyMatch(Material::isAir)) {
                throw new IllegalArgumentException("RecipeChoice.MaterialChoice cannot contain air");
            }
            return this;
        }
    }

    /** Matches an item type and its metadata. */
    class ExactChoice implements RecipeChoice {
        private final ItemStack stack;
        public ExactChoice(ItemStack stack) { this.stack = stack == null ? new ItemStack(Material.AIR) : stack.clone(); }
        @Override public boolean test(ItemStack candidate) { return stack.isSimilar(candidate); }
        public List<ItemStack> getChoices() { return Collections.singletonList(getItemStack()); }
        @Override public ItemStack getItemStack() { return stack.clone(); }
        @Override public ExactChoice clone() { return new ExactChoice(stack); }
        @Override public ExactChoice validate(boolean allowEmpty) {
            if (stack.getType().isAir()) {
                throw new IllegalArgumentException("RecipeChoice.ExactChoice cannot contain air");
            }
            return this;
        }
    }
}
