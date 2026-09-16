/** Exact Paper BrewEvent API and snapshot bridge contract. */
final class BrewingCheck {
    private static final String SLOT = "\u001e";
    private static final String SECTION = "\u001f";

    private BrewingCheck() { }

    static void check() throws Exception {
        apiShape();
        basePotionRoundTrip();

        org.bukkit.plugin.Plugin owner = foton.PluginHost.all()[0];
        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() { };
        int[] calls = {0};
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.inventory.BrewEvent.class,
            listener,
            org.bukkit.event.EventPriority.NORMAL,
            (registered, raw) -> {
                org.bukkit.event.inventory.BrewEvent event =
                    (org.bukkit.event.inventory.BrewEvent) raw;
                calls[0]++;
                Checks.same(event.getContents().getSize(), 5,
                    "BrewEvent should expose all five brewing slots");
                Checks.same(event.getFuelLevel(), 7,
                    "BrewEvent fuel should be captured at completion");
                Checks.expect(event.getContents().getHolder() instanceof org.bukkit.block.BrewingStand,
                    "BrewerInventory holder should be a BrewingStand");
                event.getContents().setIngredient(new org.bukkit.inventory.ItemStack(
                    org.bukkit.Material.REDSTONE, 2));
                event.getContents().setFuel(new org.bukkit.inventory.ItemStack(
                    org.bukkit.Material.DIRT));
                event.getResults().set(0, new org.bukkit.inventory.ItemStack(
                    org.bukkit.Material.SUGAR));
                event.getResults().subList(1, event.getResults().size()).clear();
                if (calls[0] == 1) event.setCancelled(true);
            },
            owner,
            false);

        String contents = String.join(SLOT,
            potion(org.bukkit.potion.PotionType.WATER),
            potion(org.bukkit.potion.PotionType.WATER),
            potion(org.bukkit.potion.PotionType.WATER),
            "minecraft:nether_wart 1",
            "minecraft:blaze_powder 1");
        String results = String.join(SLOT,
            potion(org.bukkit.potion.PotionType.AWKWARD),
            potion(org.bukkit.potion.PotionType.AWKWARD),
            potion(org.bukkit.potion.PotionType.AWKWARD));

        String cancelled = foton.EventBridge.fireBrew(
            "minecraft:overworld", 3, 64, -2, contents, results, 7);
        Checks.expect(cancelled.startsWith("1" + SECTION),
            "cancelled BrewEvent should cross the bridge");

        String committed = foton.EventBridge.fireBrew(
            "minecraft:overworld", 3, 64, -2, contents, results, 7);
        String[] sections = committed.split(SECTION, -1);
        Checks.same(sections.length, 3, "BrewEvent bridge response section count");
        Checks.same(sections[0], "0", "successful BrewEvent cancellation flag");
        String[] mutatedContents = sections[1].split(SLOT, -1);
        Checks.same(mutatedContents.length, 5, "BrewEvent inventory snapshot length");
        Checks.expect(mutatedContents[3].startsWith("minecraft:redstone 2"),
            "ingredient snapshot mutation was lost");
        Checks.expect(mutatedContents[4].startsWith("minecraft:dirt 1"),
            "fuel-slot snapshot mutation was lost");
        String[] mutatedResults = sections[2].split(SLOT, -1);
        Checks.same(mutatedResults.length, 1,
            "a listener-shortened BrewEvent result list was padded");
        Checks.expect(mutatedResults[0].startsWith("minecraft:sugar 1"),
            "mutable BrewEvent result was lost");

        foton.EventBridge.unregister(listener);
    }

    private static void apiShape() throws Exception {
        Checks.expect(org.bukkit.block.Container.class.isAssignableFrom(
            org.bukkit.block.BrewingStand.class),
            "BrewingStand should extend Container");
        Checks.same(org.bukkit.inventory.BrewerInventory.class
                .getMethod("getHolder").getReturnType(),
            org.bukkit.block.BrewingStand.class,
            "BrewerInventory covariant holder type");
        org.bukkit.event.inventory.BrewEvent.class.getConstructor(
            org.bukkit.block.Block.class,
            org.bukkit.inventory.BrewerInventory.class,
            java.util.List.class,
            int.class);
        org.bukkit.block.BrewingStand.class.getMethod("getBrewingTime");
        org.bukkit.block.BrewingStand.class.getMethod("setBrewingTime", int.class);
        org.bukkit.block.BrewingStand.class.getMethod("getRecipeBrewTime");
        org.bukkit.block.BrewingStand.class.getMethod("setRecipeBrewTime", int.class);
        org.bukkit.block.BrewingStand.class.getMethod("getFuelLevel");
        org.bukkit.block.BrewingStand.class.getMethod("setFuelLevel", int.class);
        org.bukkit.block.BrewingStand.class.getMethod("getSnapshotInventory");
    }

    private static void basePotionRoundTrip() {
        org.bukkit.inventory.ItemStack water = new org.bukkit.inventory.ItemStack(
            org.bukkit.Material.POTION);
        org.bukkit.inventory.meta.PotionMeta meta =
            (org.bukkit.inventory.meta.PotionMeta) water.getItemMeta();
        meta.setBasePotionType(org.bukkit.potion.PotionType.WATER);
        water.setItemMeta(meta);
        org.bukkit.inventory.ItemStack decoded = foton.FotonInventory.decode(
            foton.FotonInventory.encode(water));
        org.bukkit.inventory.meta.PotionMeta decodedMeta =
            (org.bukkit.inventory.meta.PotionMeta) decoded.getItemMeta();
        Checks.same(decodedMeta.getBasePotionType(), org.bukkit.potion.PotionType.WATER,
            "item codec lost base potion identity");
    }

    private static String potion(org.bukkit.potion.PotionType type) {
        org.bukkit.inventory.ItemStack item = new org.bukkit.inventory.ItemStack(
            org.bukkit.Material.POTION);
        org.bukkit.inventory.meta.PotionMeta meta =
            (org.bukkit.inventory.meta.PotionMeta) item.getItemMeta();
        meta.setBasePotionType(type);
        item.setItemMeta(meta);
        return foton.FotonInventory.encode(item);
    }
}
