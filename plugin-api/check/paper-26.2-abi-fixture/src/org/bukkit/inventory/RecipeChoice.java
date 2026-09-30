package org.bukkit.inventory;

import java.util.List;
import org.bukkit.Material;

/** Minimal Paper 26.2 constructor ABI fixture. */
public interface RecipeChoice {
    final class MaterialChoice {
        public MaterialChoice(Material choice) {}
    }

    final class ExactChoice {
        public ExactChoice(ItemStack[] stacks) {}
        public ExactChoice(List<ItemStack> stacks) {}
    }
}
