package org.bukkit.inventory;

import java.util.ArrayList;
import java.util.Arrays;
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

        public MaterialChoice(Material choice) {
            this(new Material[] {choice});
        }

        public MaterialChoice(Material... choices) {
            this(choices == null ? null : Arrays.asList(choices));
        }

        public MaterialChoice(org.bukkit.Tag<Material> tag) {
            this(tag == null ? null : new ArrayList<>(tag.getValues()));
        }

        public MaterialChoice(List<Material> choices) {
            if (choices == null) throw new IllegalArgumentException("choices");
            if (choices.isEmpty()) throw new IllegalArgumentException("Must have at least one choice");
            for (Material choice : choices) {
                if (choice == null) throw new IllegalArgumentException("Cannot have null choice");
                if (choice.isAir()) throw new IllegalArgumentException("Cannot have empty/air choice");
            }
            this.choices = new ArrayList<>(choices);
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
        private final List<ItemStack> choices;

        public ExactChoice(ItemStack stack) {
            this(new ItemStack[] {stack});
        }

        public ExactChoice(ItemStack... stacks) {
            this(stacks == null ? null : Arrays.asList(stacks));
        }

        public ExactChoice(List<ItemStack> choices) {
            if (choices == null) throw new IllegalArgumentException("choices");
            if (choices.isEmpty()) throw new IllegalArgumentException("Must have at least one choice");
            for (ItemStack choice : choices) {
                if (choice == null) throw new IllegalArgumentException("Cannot have null choice");
                if (choice.getType().isAir()) {
                    throw new IllegalArgumentException("Cannot have empty/air choice");
                }
            }
            this.choices = copyChoices(choices);
        }

        @Override public boolean test(ItemStack candidate) {
            for (ItemStack choice : choices) {
                if (choice.isSimilar(candidate)) return true;
            }
            return false;
        }

        public List<ItemStack> getChoices() {
            return Collections.unmodifiableList(copyChoices(choices));
        }

        @Override public ItemStack getItemStack() { return choices.get(0).clone(); }
        @Override public ExactChoice clone() { return new ExactChoice(choices); }
        @Override public ExactChoice validate(boolean allowEmpty) {
            if (choices.stream().anyMatch(stack -> stack.getType().isAir())) {
                throw new IllegalArgumentException("RecipeChoice.ExactChoice cannot contain air");
            }
            return this;
        }

        private static List<ItemStack> copyChoices(List<ItemStack> choices) {
            List<ItemStack> copies = new ArrayList<>(choices.size());
            for (ItemStack choice : choices) copies.add(choice.clone());
            return copies;
        }
    }
}
