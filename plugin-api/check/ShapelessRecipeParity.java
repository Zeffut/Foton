import java.util.List;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.RecipeChoice;
import org.bukkit.inventory.ShapelessRecipe;

/** Compile against the pinned Paper 1.21.11 API to verify Zelda's exact call descriptor. */
public final class ShapelessRecipeParity {
    private ShapelessRecipeParity() {}

    public static void main(String[] args) {
        check();
        System.out.println("Paper-compiled counted shapeless recipe passed");
    }

    static void check() {
        NamespacedKey key = new NamespacedKey("foton", "herbe_counted_parity");
        ShapelessRecipe recipe = new ShapelessRecipe(key, new ItemStack(Material.BREAD));
        if (recipe.addIngredient(Material.WHEAT).addIngredient(3, Material.SUGAR) != recipe) {
            throw new AssertionError("counted addIngredient must chain on the same recipe");
        }
        if (!key.equals(recipe.getKey()) || recipe.getResult().getType() != Material.BREAD) {
            throw new AssertionError("counted ingredients must not replace recipe identity or result");
        }
        assertIngredients(recipe, 4, Material.WHEAT, Material.SUGAR, Material.SUGAR, Material.SUGAR);

        List<RecipeChoice> choices = recipe.getChoiceList();
        choices.clear();
        List<ItemStack> stacks = recipe.getIngredientList();
        stacks.get(0).setType(Material.STONE);
        stacks.clear();
        assertIngredients(recipe, 4, Material.WHEAT, Material.SUGAR, Material.SUGAR, Material.SUGAR);

        ItemStack sourceStack = new ItemStack(Material.APPLE);
        RecipeChoice.ExactChoice sourceChoice = new RecipeChoice.ExactChoice(sourceStack);
        ShapelessRecipe copied = new ShapelessRecipe(
            new NamespacedKey("foton", "choice_copy_parity"), new ItemStack(Material.BREAD));
        copied.addIngredient(sourceChoice);
        sourceStack.setType(Material.CARROT);
        sourceChoice.getChoices().get(0).setType(Material.POTATO);
        if (copied.getChoiceList().get(0).getItemStack().getType() != Material.APPLE) {
            throw new AssertionError("adding a choice must clone its item stack");
        }
        RecipeChoice.ExactChoice readChoice =
            (RecipeChoice.ExactChoice) copied.getChoiceList().get(0);
        readChoice.getChoices().get(0).setType(Material.STONE);
        if (copied.getChoiceList().get(0).getItemStack().getType() != Material.APPLE) {
            throw new AssertionError("reading choices must clone nested mutable item stacks");
        }
        RecipeChoice.MaterialChoice materialChoice =
            (RecipeChoice.MaterialChoice) recipe.getChoiceList().get(0);
        if (materialChoice == recipe.getChoiceList().get(0)
                || materialChoice.clone() == materialChoice) {
            throw new AssertionError("material choices must be independently cloned");
        }

        recipe.addIngredient(0, (Material) null).addIngredient(-2, (Material) null);
        assertIngredients(recipe, 4, Material.WHEAT, Material.SUGAR, Material.SUGAR, Material.SUGAR);
        rejects(() -> recipe.addIngredient(6, Material.SUGAR));
        rejects(() -> recipe.addIngredient(1, (Material) null));
        rejects(() -> recipe.addIngredient(1, Material.AIR));
        rejects(() -> recipe.addIngredient(1, Material.OAK_WALL_SIGN));
        assertIngredients(recipe, 4, Material.WHEAT, Material.SUGAR, Material.SUGAR, Material.SUGAR);

        recipe.addIngredient(5, Material.SUGAR);
        assertIngredients(recipe, 9, Material.WHEAT, Material.SUGAR, Material.SUGAR,
            Material.SUGAR, Material.SUGAR, Material.SUGAR, Material.SUGAR,
            Material.SUGAR, Material.SUGAR);
        rejects(() -> recipe.addIngredient(1, Material.SUGAR));
        if (recipe.getChoiceList().size() != 9) {
            throw new AssertionError("rejected tenth ingredient must leave recipe unchanged");
        }
    }

    private static void assertIngredients(ShapelessRecipe recipe, int expected, Material... types) {
        List<RecipeChoice> choices = recipe.getChoiceList();
        List<ItemStack> stacks = recipe.getIngredientList();
        if (choices.size() != expected || stacks.size() != expected || types.length != expected) {
            throw new AssertionError("wrong counted ingredient list length");
        }
        for (int i = 0; i < expected; i++) {
            if (choices.get(i).getItemStack().getType() != types[i] || stacks.get(i).getType() != types[i]) {
                throw new AssertionError("wrong ingredient at grid index " + i);
            }
        }
    }

    private static void rejects(Runnable action) {
        try {
            action.run();
        } catch (IllegalArgumentException expected) {
            return;
        }
        throw new AssertionError("Paper rejects invalid shapeless ingredients");
    }
}
