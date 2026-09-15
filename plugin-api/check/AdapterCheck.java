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
        resolvableProfileBuilderReturnsItsConcreteProfile();
        namespacedKeyImplementsAdventureKeyWithoutChangingItsOwnIdentityRules();
    }

    static void check() throws Exception {
        equipmentSlotDispatchesToItsNamedLiveSlot();
        pluginConfigMethodsOperateThroughTheLivePluginConfig();
        worldClassFilterUsesTheExistingEntityCollection();
        registryIteratorDelegatesTheCompleteStreamIteratorContract();
        shapelessRecipePreservesStackIngredientMultiplicity();
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
