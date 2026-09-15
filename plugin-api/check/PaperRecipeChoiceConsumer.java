import java.util.List;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.RecipeChoice;

/** Compiled against the Paper fixture and linked against Foton at runtime. */
public final class PaperRecipeChoiceConsumer {
    private PaperRecipeChoiceConsumer() {}

    public static void invokeConstructors() {
        new RecipeChoice.MaterialChoice(Material.STONE);
        new RecipeChoice.ExactChoice(new ItemStack[] {new ItemStack(Material.STONE)});
        new RecipeChoice.ExactChoice(List.of(new ItemStack(Material.DIRT)));
    }
}
