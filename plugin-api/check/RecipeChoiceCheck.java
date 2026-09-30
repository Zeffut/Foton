import java.util.ArrayList;
import java.util.List;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.RecipeChoice;

/** Focused validation and defensive-copy checks for Paper recipe choices. */
final class RecipeChoiceCheck {
    private RecipeChoiceCheck() {}

    static void check() {
        rejectsInvalidMaterialChoicesWithoutTouchingSources();
        materialChoicesDetachSourcesReturnsAndClones();
        rejectsInvalidExactChoicesWithoutTouchingSources();
        exactChoicesDeeplyDetachSourcesReturnsAndClones();
    }

    private static void rejectsInvalidMaterialChoicesWithoutTouchingSources() {
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice((Material) null),
            "a null scalar material");
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice(Material.AIR),
            "an AIR scalar material");
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice((Material[]) null),
            "a null material array");
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice(new Material[0]),
            "an empty material array");
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice((List<Material>) null),
            "a null material list");
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice(List.of()),
            "an empty material list");

        Material[] array = {Material.STONE, null};
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice(array),
            "a null material array element");
        Checks.same(array[0], Material.STONE,
            "rejected material array retained its valid prefix");
        Checks.expect(array[1] == null,
            "rejected material array retained its invalid element");

        ArrayList<Material> list = new ArrayList<>(List.of(Material.STONE, Material.AIR));
        expectIllegalArgument(() -> new RecipeChoice.MaterialChoice(list),
            "an AIR material list element");
        Checks.same(list, List.of(Material.STONE, Material.AIR),
            "rejected material list remained untouched");
    }

    private static void materialChoicesDetachSourcesReturnsAndClones() {
        ArrayList<Material> source = new ArrayList<>(List.of(Material.STONE, Material.DIRT));
        RecipeChoice.MaterialChoice choice = new RecipeChoice.MaterialChoice(source);
        source.set(0, Material.COBBLESTONE);
        Checks.same(choice.getChoices(), List.of(Material.STONE, Material.DIRT),
            "material choice detached its source list");
        expectUnsupported(() -> choice.getChoices().set(0, Material.COBBLESTONE),
            "material choice return list");

        RecipeChoice.MaterialChoice clone = choice.clone();
        Checks.expect(clone != choice, "material choice clone must be a new object");
        Checks.same(clone.getChoices(), choice.getChoices(),
            "material choice clone retained all choices");
    }

    private static void rejectsInvalidExactChoicesWithoutTouchingSources() {
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice((ItemStack) null),
            "a null scalar stack");
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice(new ItemStack(Material.AIR)),
            "an AIR scalar stack");
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice((ItemStack[]) null),
            "a null stack array");
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice(new ItemStack[0]),
            "an empty stack array");
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice((List<ItemStack>) null),
            "a null stack list");
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice(List.of()),
            "an empty stack list");

        ItemStack valid = new ItemStack(Material.STONE, 2);
        ItemStack[] array = {valid, null};
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice(array),
            "a null stack array element");
        Checks.expect(array[0] == valid && array[1] == null,
            "rejected stack array remained untouched");

        ItemStack air = new ItemStack(Material.AIR);
        ArrayList<ItemStack> list = new ArrayList<>(List.of(valid, air));
        expectIllegalArgument(() -> new RecipeChoice.ExactChoice(list),
            "an AIR stack list element");
        Checks.expect(list.get(0) == valid && list.get(1) == air,
            "rejected stack list remained untouched");
    }

    private static void exactChoicesDeeplyDetachSourcesReturnsAndClones() {
        ItemStack first = new ItemStack(Material.STONE, 2);
        ItemStack second = new ItemStack(Material.DIRT, 3);
        ArrayList<ItemStack> source = new ArrayList<>(List.of(first, second));
        RecipeChoice.ExactChoice choice = new RecipeChoice.ExactChoice(source);

        first.setType(Material.COBBLESTONE);
        second.setAmount(8);
        source.clear();
        assertStacks(choice.getChoices(), Material.STONE, 2, Material.DIRT, 3,
            "exact choice detached its source stacks and collection");

        List<ItemStack> returned = choice.getChoices();
        expectUnsupported(() -> returned.add(new ItemStack(Material.COBBLESTONE)),
            "exact choice return list");
        returned.get(0).setType(Material.COBBLESTONE);
        returned.get(1).setAmount(9);
        assertStacks(choice.getChoices(), Material.STONE, 2, Material.DIRT, 3,
            "exact choice detached returned stacks");

        RecipeChoice.ExactChoice clone = choice.clone();
        List<ItemStack> cloneView = clone.getChoices();
        cloneView.get(0).setType(Material.COBBLESTONE);
        cloneView.get(1).setAmount(10);
        assertStacks(choice.getChoices(), Material.STONE, 2, Material.DIRT, 3,
            "exact choice clone did not share stack state");
        Checks.expect(choice.test(new ItemStack(Material.STONE, 64))
                && choice.test(new ItemStack(Material.DIRT, 1)),
            "exact choice matched every stored stack independently of amount");
    }

    private static void assertStacks(List<ItemStack> stacks,
            Material firstType, int firstAmount, Material secondType, int secondAmount,
            String description) {
        Checks.same(stacks.size(), 2, description + " size");
        Checks.same(stacks.get(0).getType(), firstType, description + " first type");
        Checks.same(stacks.get(0).getAmount(), firstAmount, description + " first amount");
        Checks.same(stacks.get(1).getType(), secondType, description + " second type");
        Checks.same(stacks.get(1).getAmount(), secondAmount, description + " second amount");
    }

    private static void expectIllegalArgument(Runnable operation, String description) {
        try {
            operation.run();
            throw new AssertionError(description + " must be rejected");
        } catch (IllegalArgumentException expected) {
            // Expected Paper validation.
        }
    }

    private static void expectUnsupported(Runnable operation, String description) {
        try {
            operation.run();
            throw new AssertionError(description + " must be immutable");
        } catch (UnsupportedOperationException expected) {
            // Expected defensive view.
        }
    }
}
