import org.bukkit.Material;
import org.bukkit.inventory.EntityEquipment;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** Focused compatibility checks for adapters backed by existing API state. */
final class AdapterCheck {
    private AdapterCheck() {}

    public static void main(String[] args) throws Exception {
        equipmentSlotDispatchesToItsNamedLiveSlot();
        pluginConfigMethodsOperateThroughTheLivePluginConfig();
        worldClassFilterUsesTheExistingEntityCollection();
        registryIteratorDelegatesTheCompleteStreamIteratorContract();
        shapelessRecipePreservesStackIngredientMultiplicity();
        shapelessRecipeRejectsInvalidMaterialIngredientsAtomically();
        shapelessRecipeRejectsInvalidItemStackIngredientsAtomically();
        shapelessRecipeChoiceMatchesPaperValidationAndCloneSemantics();
        resolvableProfileBuilderReturnsItsConcreteProfile();
        namespacedKeyImplementsAdventureKeyWithoutChangingItsOwnIdentityRules();
    }

    static void check() throws Exception {
        equipmentSlotDispatchesToItsNamedLiveSlot();
        pluginConfigMethodsOperateThroughTheLivePluginConfig();
        worldClassFilterUsesTheExistingEntityCollection();
        registryIteratorDelegatesTheCompleteStreamIteratorContract();
        shapelessRecipePreservesStackIngredientMultiplicity();
        shapelessRecipeRejectsInvalidMaterialIngredientsAtomically();
        shapelessRecipeRejectsInvalidItemStackIngredientsAtomically();
        shapelessRecipeChoiceMatchesPaperValidationAndCloneSemantics();
        resolvableProfileBuilderReturnsItsConcreteProfile();
        namespacedKeyImplementsAdventureKeyWithoutChangingItsOwnIdentityRules();
    }

    private static void equipmentSlotDispatchesToItsNamedLiveSlot() {
        RecordingEquipment equipment = new RecordingEquipment();
        ItemStack hand = new ItemStack(Material.STONE);
        ItemStack offHand = new ItemStack(Material.DIRT);
        ItemStack boots = new ItemStack(Material.LEATHER_BOOTS);
        ItemStack leggings = new ItemStack(Material.LEATHER_LEGGINGS);
        ItemStack chestplate = new ItemStack(Material.LEATHER_CHESTPLATE);
        ItemStack helmet = new ItemStack(Material.LEATHER_HELMET);

        equipment.setItem(EquipmentSlot.HAND, hand);
        equipment.setItem(EquipmentSlot.OFF_HAND, offHand);
        equipment.setItem(EquipmentSlot.FEET, boots);
        equipment.setItem(EquipmentSlot.LEGS, leggings);
        equipment.setItem(EquipmentSlot.CHEST, chestplate);
        equipment.setItem(EquipmentSlot.HEAD, helmet);

        same(equipment.getItem(EquipmentSlot.HAND), hand, "main hand slot");
        same(equipment.getItem(EquipmentSlot.OFF_HAND), offHand, "off-hand slot");
        same(equipment.getItem(EquipmentSlot.FEET), boots, "boots slot");
        same(equipment.getItem(EquipmentSlot.LEGS), leggings, "leggings slot");
        same(equipment.getItem(EquipmentSlot.CHEST), chestplate, "chestplate slot");
        same(equipment.getItem(EquipmentSlot.HEAD), helmet, "helmet slot");
    }

    private static void pluginConfigMethodsOperateThroughTheLivePluginConfig()
            throws Exception {
        java.io.File folder = java.nio.file.Files.createTempDirectory("foton-plugin-config")
            .toFile();
        try {
            java.io.File configFile = new java.io.File(folder, "config.yml");
            java.nio.file.Files.writeString(configFile.toPath(), "saved:\n  value: 1\n");
            ConfigPlugin implementation = new ConfigPlugin();
            implementation.init(null, new org.bukkit.plugin.PluginDescriptionFile(
                new java.io.StringReader("name: AdapterCheck\nversion: 1\nmain: AdapterCheck\n")),
                folder);
            org.bukkit.plugin.Plugin plugin = implementation;

            plugin.reloadConfig();
            plugin.getConfig().set("saved.value", 7);
            plugin.saveConfig();
            equal(org.bukkit.configuration.file.YamlConfiguration.loadConfiguration(
                configFile).getInt("saved.value"), 7,
                "Plugin.saveConfig writes the live configuration");

            java.nio.file.Files.writeString(configFile.toPath(), "saved:\n  value: 9\n");
            plugin.reloadConfig();
            equal(plugin.getConfig().getInt("saved.value"), 9,
                "Plugin.reloadConfig refreshes the live configuration");
        } finally {
            java.nio.file.Files.deleteIfExists(new java.io.File(folder, "config.yml").toPath());
            java.nio.file.Files.deleteIfExists(folder.toPath());
        }
    }

    private static void worldClassFilterUsesTheExistingEntityCollection() {
        org.bukkit.entity.Entity player = new foton.FotonPlayer(java.util.UUID.randomUUID());
        org.bukkit.entity.Entity other = new foton.FotonEntity(java.util.UUID.randomUUID());
        org.bukkit.World world = (org.bukkit.World) java.lang.reflect.Proxy.newProxyInstance(
            AdapterCheck.class.getClassLoader(), new Class<?>[] { org.bukkit.World.class },
            (proxy, method, arguments) -> {
                if (method.getName().equals("getEntities")) return java.util.List.of(player, other);
                if (method.isDefault()) {
                    return java.lang.reflect.InvocationHandler.invokeDefault(proxy, method, arguments);
                }
                throw new UnsupportedOperationException(method.getName());
            });

        java.util.Collection<foton.FotonPlayer> players =
            world.getEntitiesByClass(foton.FotonPlayer.class);
        equal(players.size(), 1, "class filtering keeps only matching entities");
        same(players.iterator().next(), player, "class filtering preserves the live entity");
    }

    private static void registryIteratorDelegatesTheCompleteStreamIteratorContract() {
        java.util.Iterator<Material> iterator = org.bukkit.Registry.MATERIAL.iterator();
        expect(iterator.hasNext(), "registry iterator starts with a value");
        Material first = iterator.next();
        expect(first != null, "registry iterator returns its live stream value");
        java.util.concurrent.atomic.AtomicInteger remaining = new java.util.concurrent.atomic.AtomicInteger();
        iterator.forEachRemaining(ignored -> remaining.incrementAndGet());
        expect(remaining.get() > 0, "registry iterator forwards forEachRemaining");
        expect(!iterator.hasNext(), "registry iterator reports exhaustion");
        boolean exhausted = false;
        try {
            iterator.next();
        } catch (java.util.NoSuchElementException expected) {
            exhausted = true;
        }
        expect(exhausted, "registry iterator throws after exhaustion");
    }

    private static void shapelessRecipePreservesStackIngredientMultiplicity() {
        org.bukkit.inventory.ShapelessRecipe recipe = new org.bukkit.inventory.ShapelessRecipe(
            org.bukkit.NamespacedKey.minecraft("adapter_check"), new ItemStack(Material.STONE));
        recipe.addIngredient(new ItemStack(Material.DIRT, 3));
        recipe.addIngredient(2, Material.COBBLESTONE);

        equal(recipe.getChoiceList().size(), 5,
            "a stack ingredient retains its requested multiplicity");
        for (org.bukkit.inventory.RecipeChoice choice : recipe.getChoiceList().subList(0, 3)) {
            expect(choice.test(new ItemStack(Material.DIRT)),
                "each repeated ingredient delegates to the requested choice");
        }
        for (org.bukkit.inventory.RecipeChoice choice : recipe.getChoiceList().subList(3, 5)) {
            expect(choice.test(new ItemStack(Material.COBBLESTONE)),
                "the explicit count retains every requested ingredient");
        }
    }

    /**
     * Catches the mutations that silently add null/AIR choices or append part
     * of an over-limit batch before rejecting the rest.
     */
    private static void shapelessRecipeRejectsInvalidMaterialIngredientsAtomically() {
        org.bukkit.inventory.ShapelessRecipe recipe = new org.bukkit.inventory.ShapelessRecipe(
            org.bukkit.NamespacedKey.minecraft("adapter_validation"), new ItemStack(Material.STONE));
        recipe.addIngredient(Material.DIRT);

        assertRejectedMaterialAdditionLeavesChoicesUntouched(recipe, null,
            "null material must be rejected as Paper rejects an invalid material choice");
        assertRejectedMaterialAdditionLeavesChoicesUntouched(recipe, Material.AIR,
            "AIR material must be rejected as Paper rejects an empty material choice");

        for (int ingredient = recipe.getChoiceList().size(); ingredient < 8; ingredient++) {
            recipe.addIngredient(Material.COBBLESTONE);
        }
        java.util.List<org.bukkit.inventory.RecipeChoice> beforeOverflow =
            java.util.List.copyOf(recipe.getChoiceList());
        boolean overflowRejected = false;
        try {
            recipe.addIngredient(2, Material.COBBLESTONE);
        } catch (IllegalArgumentException expected) {
            equal(expected.getClass(), IllegalArgumentException.class,
                "the over-limit batch uses Paper's IllegalArgumentException");
            overflowRejected = true;
        }
        expect(overflowRejected,
            "a batch exceeding the global nine-ingredient limit throws IllegalArgumentException");
        sameChoices(recipe.getChoiceList(), beforeOverflow,
            "an over-limit batch leaves every existing choice untouched");
    }

    private static void assertRejectedMaterialAdditionLeavesChoicesUntouched(
            org.bukkit.inventory.ShapelessRecipe recipe, Material material, String failure) {
        java.util.List<org.bukkit.inventory.RecipeChoice> before =
            java.util.List.copyOf(recipe.getChoiceList());
        boolean rejected = false;
        try {
            recipe.addIngredient(1, material);
        } catch (IllegalArgumentException expected) {
            equal(expected.getClass(), IllegalArgumentException.class,
                "the invalid material uses Paper's IllegalArgumentException");
            rejected = true;
        }
        expect(rejected, failure);
        sameChoices(recipe.getChoiceList(), before, failure + " without altering choices");
    }

    /**
     * Catches the mutations that accept null/AIR stack choices, skip a stack's
     * amount in the global limit, or partially append an over-limit batch.
     */
    private static void shapelessRecipeRejectsInvalidItemStackIngredientsAtomically() {
        org.bukkit.inventory.ShapelessRecipe recipe = new org.bukkit.inventory.ShapelessRecipe(
            org.bukkit.NamespacedKey.minecraft("adapter_stack_validation"),
            new ItemStack(Material.STONE));

        assertRejectedStackAdditionLeavesChoicesUntouched(recipe, 1, null,
            NullPointerException.class,
            "a counted null ItemStack must dereference as Paper does");
        assertRejectedStackAdditionLeavesChoicesUntouched(recipe, null,
            NullPointerException.class,
            "an uncounted null ItemStack must dereference its amount as Paper does");
        assertRejectedStackAdditionLeavesChoicesUntouched(recipe, 1,
            new ItemStack(Material.AIR), IllegalArgumentException.class,
            "an AIR ItemStack must be rejected before an exact choice is added");

        for (int ingredient = 0; ingredient < 8; ingredient++) {
            recipe.addIngredient(Material.COBBLESTONE);
        }
        assertRejectedStackAdditionLeavesChoicesUntouched(recipe, 2,
            new ItemStack(Material.DIRT), IllegalArgumentException.class,
            "a counted ItemStack batch over the global limit must be atomic");
        assertRejectedStackAdditionLeavesChoicesUntouched(recipe, 2, null,
            IllegalArgumentException.class,
            "the global limit must run before a counted ItemStack is dereferenced");
        assertRejectedStackAdditionLeavesChoicesUntouched(recipe,
            new ItemStack(Material.DIRT, 2), IllegalArgumentException.class,
            "the uncounted ItemStack overload must honor its stack amount and global limit");
    }

    /** Verifies RecipeChoice's own Paper validation, limit, and clone contract. */
    private static void shapelessRecipeChoiceMatchesPaperValidationAndCloneSemantics() {
        org.bukkit.inventory.ShapelessRecipe recipe = new org.bukkit.inventory.ShapelessRecipe(
            org.bukkit.NamespacedKey.minecraft("adapter_choice_validation"),
            new ItemStack(Material.STONE));

        java.util.List<org.bukkit.inventory.RecipeChoice> beforeNull =
            java.util.List.copyOf(recipe.getChoiceList());
        boolean nullRejected = false;
        try {
            recipe.addIngredient((org.bukkit.inventory.RecipeChoice) null);
        } catch (NullPointerException expected) {
            nullRejected = true;
        }
        expect(nullRejected, "a null RecipeChoice must dereference as Paper does");
        sameChoices(recipe.getChoiceList(), beforeNull,
            "a null RecipeChoice leaves choices untouched");

        MutableRecipeChoice choice = new MutableRecipeChoice(Material.DIRT);
        recipe.addIngredient(choice);
        org.bukkit.inventory.RecipeChoice stored = recipe.getChoiceList().get(0);
        expect(stored != choice, "a RecipeChoice is cloned before storage");
        choice.setMaterial(Material.COBBLESTONE);
        expect(stored.test(new ItemStack(Material.DIRT)),
            "a stored RecipeChoice is independent from its caller-owned choice");
        expect(!stored.test(new ItemStack(Material.COBBLESTONE)),
            "a cloned RecipeChoice does not retain later caller mutations");

        for (int ingredient = recipe.getChoiceList().size(); ingredient < 9; ingredient++) {
            recipe.addIngredient(Material.STONE);
        }
        java.util.List<org.bukkit.inventory.RecipeChoice> beforeOverflow =
            java.util.List.copyOf(recipe.getChoiceList());
        boolean overflowRejected = false;
        try {
            recipe.addIngredient(new MutableRecipeChoice(Material.DIRT));
        } catch (IllegalArgumentException expected) {
            overflowRejected = true;
        }
        expect(overflowRejected, "a tenth RecipeChoice must be rejected");
        sameChoices(recipe.getChoiceList(), beforeOverflow,
            "an over-limit RecipeChoice leaves choices untouched");
    }

    private static void assertRejectedStackAdditionLeavesChoicesUntouched(
            org.bukkit.inventory.ShapelessRecipe recipe, int count, ItemStack stack,
            Class<? extends RuntimeException> expectedType, String failure) {
        java.util.List<org.bukkit.inventory.RecipeChoice> before =
            java.util.List.copyOf(recipe.getChoiceList());
        try {
            recipe.addIngredient(count, stack);
            throw new AssertionError(failure + " did not throw");
        } catch (RuntimeException expected) {
            equal(expected.getClass(), expectedType, failure + " exception type");
        }
        sameChoices(recipe.getChoiceList(), before, failure + " without altering choices");
    }

    private static void assertRejectedStackAdditionLeavesChoicesUntouched(
            org.bukkit.inventory.ShapelessRecipe recipe, ItemStack stack,
            Class<? extends RuntimeException> expectedType, String failure) {
        java.util.List<org.bukkit.inventory.RecipeChoice> before =
            java.util.List.copyOf(recipe.getChoiceList());
        try {
            recipe.addIngredient(stack);
            throw new AssertionError(failure + " did not throw");
        } catch (RuntimeException expected) {
            equal(expected.getClass(), expectedType, failure + " exception type");
        }
        sameChoices(recipe.getChoiceList(), before, failure + " without altering choices");
    }

    private static void sameChoices(java.util.List<org.bukkit.inventory.RecipeChoice> actual,
            java.util.List<org.bukkit.inventory.RecipeChoice> expected, String description) {
        equal(actual.size(), expected.size(), description + " size");
        for (int index = 0; index < expected.size(); index++) {
            same(actual.get(index), expected.get(index), description + " at index " + index);
        }
    }

    private static void resolvableProfileBuilderReturnsItsConcreteProfile() {
        java.util.UUID id = java.util.UUID.randomUUID();
        io.papermc.paper.datacomponent.item.ResolvableProfile profile =
            io.papermc.paper.datacomponent.item.ResolvableProfile.resolvableProfile()
                .uuid(id)
                .name("AdapterCheck")
                .build();
        equal(profile.uuid(), id, "profile builder returns the live profile uuid");
        equal(profile.name(), "AdapterCheck", "profile builder returns the live profile name");
    }

    private static void namespacedKeyImplementsAdventureKeyWithoutChangingItsOwnIdentityRules() {
        org.bukkit.NamespacedKey bukkit = new org.bukkit.NamespacedKey("foton", "adapter_check");
        net.kyori.adventure.key.Key adventure = bukkit;

        equal(adventure.namespace(), "foton", "Adventure namespace delegates to Bukkit key");
        equal(adventure.value(), "adapter_check", "Adventure value delegates to Bukkit key");
        equal(adventure.asString(), "foton:adapter_check", "Adventure string delegates to Bukkit key");
        same(adventure.key(), bukkit, "Adventure Key.key preserves Bukkit key identity");
        expect(!bukkit.equals(net.kyori.adventure.key.Key.key("foton", "adapter_check")),
            "Bukkit key equality remains limited to Bukkit keys");
        equal(org.bukkit.NamespacedKey.fromString("Foton:Mixed_Value").toString(),
            "Foton:Mixed_Value", "existing key normalization remains unchanged");
    }

    private static void same(Object actual, Object expected, String description) {
        if (actual != expected) {
            throw new AssertionError(description + " did not preserve its live item identity");
        }
    }

    private static void equal(Object actual, Object expected, String description) {
        if (expected == null ? actual != null : !expected.equals(actual)) {
            throw new AssertionError(description + ": expected " + expected + ", got " + actual);
        }
    }

    private static void expect(boolean condition, String description) {
        if (!condition) throw new AssertionError(description);
    }

    private static final class MutableRecipeChoice implements org.bukkit.inventory.RecipeChoice {
        private Material material;

        private MutableRecipeChoice(Material material) {
            this.material = material;
        }

        private void setMaterial(Material material) {
            this.material = material;
        }

        @Override public boolean test(ItemStack stack) {
            return stack != null && stack.getType() == material;
        }

        @Override public ItemStack getItemStack() {
            return new ItemStack(material);
        }

        @Override public MutableRecipeChoice clone() {
            return new MutableRecipeChoice(material);
        }
    }

    private static final class RecordingEquipment implements EntityEquipment {
        private ItemStack hand;
        private ItemStack offHand;
        private ItemStack boots;
        private ItemStack leggings;
        private ItemStack chestplate;
        private ItemStack helmet;

        @Override public ItemStack[] getArmorContents() {
            return new ItemStack[] { boots, leggings, chestplate, helmet };
        }
        @Override public void setArmorContents(ItemStack[] armor) {
            boots = armor != null && armor.length > 0 ? armor[0] : null;
            leggings = armor != null && armor.length > 1 ? armor[1] : null;
            chestplate = armor != null && armor.length > 2 ? armor[2] : null;
            helmet = armor != null && armor.length > 3 ? armor[3] : null;
        }
        @Override public ItemStack getItemInMainHand() { return hand; }
        @Override public void setItemInMainHand(ItemStack item) { hand = item; }
        @Override public ItemStack getItemInOffHand() { return offHand; }
        @Override public void setItemInOffHand(ItemStack item) { offHand = item; }
        @Override public void clear() {
            hand = offHand = boots = leggings = chestplate = helmet = null;
        }
    }

    private static final class ConfigPlugin extends org.bukkit.plugin.java.JavaPlugin {}
}
