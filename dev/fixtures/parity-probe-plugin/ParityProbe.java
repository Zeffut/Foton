package foton.fixture.parity;

import java.util.ArrayList;
import java.util.List;
import org.bukkit.Bukkit;
import org.bukkit.Location;
import org.bukkit.Material;
import org.bukkit.NamespacedKey;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.block.data.Bisected;
import org.bukkit.block.data.BlockData;
import org.bukkit.block.data.Directional;
import org.bukkit.block.data.Lightable;
import org.bukkit.block.data.Waterlogged;
import org.bukkit.block.data.type.Campfire;
import org.bukkit.block.data.type.Furnace;
import org.bukkit.block.data.type.Stairs;
import org.bukkit.entity.Entity;
import org.bukkit.entity.EntityType;
import org.bukkit.entity.Player;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.PlayerInventory;
import org.bukkit.inventory.ShapedRecipe;
import org.bukkit.inventory.ShapelessRecipe;
import org.bukkit.inventory.meta.ItemMeta;
import org.bukkit.persistence.PersistentDataType;
import org.bukkit.plugin.java.JavaPlugin;

/** A Paper-compiled plugin that checks, on a live server and a real player, the
 * behaviours ZeldaCiv relies on: canonical worlds, inventory-slot mirrors,
 * block data interfaces and plugin recipes that keep their result.
 *
 * <p>The client drives it: {@code /parityprobe} runs the checks that need only
 * the player, and {@code craft}, {@code preview} and {@code taken} bracket the
 * clicks the client makes at the crafting table. */
public final class ParityProbe extends JavaPlugin {
    private interface Check { void run() throws Exception; }

    @Override public void onEnable() {
        getCommand("parityprobe").setExecutor((sender, command, label, args) -> {
            if (!(sender instanceof Player player)) return false;
            String step = args.length == 0 ? "all" : args[0];
            switch (step) {
                case "all" -> {
                    run("worlds are canonical", () -> worlds(player));
                    run("slot items mirror the inventory", () -> mirror(player));
                    run("a mirror never overwrites a changed slot", () -> staleMirror(player));
                    // Block writes from a command wait for the next tick; a scheduled task is on it.
                    Bukkit.getScheduler().runTask(this, () -> {
                        run("block data has every property interface", () -> blockData(player));
                        System.out.println("[parity-probe] DONE blocks");
                    });
                }
                case "craft" -> run("plugin recipes register", () -> prepareCrafting(player));
                case "preview" -> run("the result slot previews the whole stack (" + args[1] + ")",
                    () -> checkStack(player.getOpenInventory().getTopInventory().getItem(0), args[1]));
                case "taken" -> run("shift-click delivers the whole stack (" + args[1] + ")",
                    () -> checkTaken(player, args[1]));
                default -> { return false; }
            }
            System.out.println("[parity-probe] DONE " + step);
            return true;
        });
        System.out.println("[parity-probe] ENABLED");
    }

    private static void run(String name, Check check) {
        try {
            check.run();
            System.out.println("[parity-probe] OK " + name);
        } catch (Throwable failure) {
            System.out.println("[parity-probe] FAIL " + name + ": " + failure);
        }
    }

    private static void require(boolean condition, String what) {
        if (!condition) throw new IllegalStateException(what);
    }

    private static void worlds(Player player) {
        World world = player.getWorld();
        Location at = player.getLocation();
        require(world == at.getWorld(), "Player#getWorld != Location#getWorld");
        require(world == Bukkit.getWorld(world.getName()), "Bukkit#getWorld is another instance");
        require(world == Bukkit.getWorlds().get(0), "Bukkit#getWorlds is another instance");
        List<Entity> stands = new ArrayList<>();
        for (int i = 1; i <= 2; i++) {
            stands.add(world.spawnEntity(at.clone().add(i, 0, 0), EntityType.ARMOR_STAND));
        }
        try {
            int viaPlayer = world.getNearbyEntities(at, 6, 6, 6).size();
            int viaLocation = at.getWorld().getNearbyEntities(at, 6, 6, 6).size();
            require(viaPlayer == viaLocation && viaPlayer >= 3,
                "World#getNearbyEntities " + viaPlayer + " vs " + viaLocation);
            List<Entity> around = player.getNearbyEntities(6, 6, 6);
            require(around.containsAll(stands), "Entity#getNearbyEntities found " + around.size());
        } finally {
            stands.forEach(Entity::remove);
        }
    }

    private void mirror(Player player) {
        PlayerInventory inventory = player.getInventory();
        inventory.clear();
        inventory.setItemInMainHand(new ItemStack(Material.BREAD, 8));
        inventory.getItemInMainHand().setAmount(7);
        require(inventory.getItemInMainHand().getAmount() == 7, "setAmount did not reach the slot");

        ItemStack hand = inventory.getItemInMainHand();
        ItemMeta meta = hand.getItemMeta();
        meta.setDisplayName("Mirrored");
        NamespacedKey key = new NamespacedKey(this, "rune");
        meta.getPersistentDataContainer().set(key, PersistentDataType.STRING, "bomb");
        hand.setItemMeta(meta);
        ItemMeta stored = inventory.getItemInMainHand().getItemMeta();
        require("Mirrored".equals(stored.getDisplayName()), "setItemMeta name did not reach the slot");
        require("bomb".equals(stored.getPersistentDataContainer().get(key, PersistentDataType.STRING)),
            "setItemMeta persistent data did not reach the slot");

        hand.setAmount(hand.getAmount() - 1);
        require(inventory.getItemInMainHand().getAmount() == 6, "a second edit did not follow the first");
        hand.setType(Material.CARROT);
        require(inventory.getItemInMainHand().getType() == Material.CARROT
            && inventory.getItemInMainHand().getAmount() == 6, "setType did not reach the slot");

        inventory.setItem(3, new ItemStack(Material.STONE, 5));
        inventory.getItem(3).setAmount(2);
        require(inventory.getItem(3).getAmount() == 2, "getItem(slot) is not a mirror");

        inventory.setItemInOffHand(new ItemStack(Material.APPLE, 4));
        inventory.getItemInOffHand().setAmount(3);
        require(inventory.getItemInOffHand().getAmount() == 3, "off hand is not a mirror");

        inventory.setHelmet(new ItemStack(Material.IRON_HELMET));
        ItemStack helmet = inventory.getHelmet();
        ItemMeta helmetMeta = helmet.getItemMeta();
        helmetMeta.setDisplayName("Cap");
        helmet.setItemMeta(helmetMeta);
        require("Cap".equals(inventory.getHelmet().getItemMeta().getDisplayName()), "armor is not a mirror");

        player.getEquipment().getItemInMainHand().setAmount(4);
        require(inventory.getItemInMainHand().getAmount() == 4, "EntityEquipment hand is not a mirror");

        ItemStack copy = inventory.getItemInMainHand().clone();
        copy.setAmount(1);
        require(inventory.getItemInMainHand().getAmount() == 4, "clone() writes back");

        inventory.getItemInMainHand().setAmount(0);
        require(inventory.getItemInMainHand().getType() == Material.AIR, "setAmount(0) did not empty the slot");
        inventory.clear();
    }

    private static void staleMirror(Player player) {
        PlayerInventory inventory = player.getInventory();
        inventory.clear();

        inventory.setItemInMainHand(new ItemStack(Material.BREAD, 8));
        ItemStack replaced = inventory.getItemInMainHand();
        inventory.setItemInMainHand(new ItemStack(Material.APPLE, 3));
        replaced.setAmount(1);
        require(inventory.getItemInMainHand().getType() == Material.APPLE
            && inventory.getItemInMainHand().getAmount() == 3, "a mirror overwrote a replaced item");

        inventory.setItemInMainHand(new ItemStack(Material.BREAD, 8));
        ItemStack moved = inventory.getItemInMainHand();
        inventory.setItem(5, moved.clone());
        inventory.setItemInMainHand(null);
        moved.setAmount(1);
        require(inventory.getItem(5).getAmount() == 8, "a mirror changed the item that moved");
        require(inventory.getItemInMainHand().getType() == Material.AIR,
            "a mirror put its item back where it no longer is");

        inventory.setItemInMainHand(new ItemStack(Material.BREAD, 8));
        ItemStack merged = inventory.getItemInMainHand();
        inventory.setItemInMainHand(new ItemStack(Material.BREAD, 12));
        merged.setAmount(7);
        require(inventory.getItemInMainHand().getAmount() == 12, "a mirror overwrote a merged stack");
        inventory.clear();
    }

    private static void blockData(Player player) {
        Location at = player.getLocation();
        int x = at.getBlockX() + 3;
        int z = at.getBlockZ();
        Block block = at.getWorld().getBlockAt(x, at.getWorld().getHighestBlockYAt(x, z) + 1, z);
        block.setType(Material.CAMPFIRE);
        BlockData campfire = block.getBlockData();
        require(campfire instanceof Campfire && campfire instanceof Lightable
            && campfire instanceof Directional && campfire instanceof Waterlogged,
            "campfire interfaces: " + campfire.getAsString() + " at " + block.getLocation());
        ((Lightable) campfire).setLit(false);
        ((Waterlogged) campfire).setWaterlogged(true);
        block.setBlockData(campfire);
        require(!((Lightable) block.getBlockData()).isLit(), "setLit did not round-trip");
        require(((Waterlogged) block.getBlockData()).isWaterlogged(), "setWaterlogged did not round-trip");

        block.setType(Material.OAK_STAIRS);
        BlockData stairs = block.getBlockData();
        require(stairs instanceof Stairs && stairs instanceof Bisected
            && stairs instanceof Directional && stairs instanceof Waterlogged, "stairs interfaces");
        ((Directional) stairs).setFacing(BlockFace.EAST);
        ((Bisected) stairs).setHalf(Bisected.Half.TOP);
        ((Waterlogged) stairs).setWaterlogged(true);
        block.setBlockData(stairs);
        BlockData read = block.getBlockData();
        require(((Directional) read).getFacing() == BlockFace.EAST
            && ((Bisected) read).getHalf() == Bisected.Half.TOP
            && ((Waterlogged) read).isWaterlogged(), "stairs state did not round-trip: " + read.getAsString());

        block.setType(Material.FURNACE);
        BlockData furnace = block.getBlockData();
        require(furnace instanceof Furnace && furnace instanceof Lightable && furnace instanceof Directional
            && !(furnace instanceof Waterlogged), "furnace interfaces");
        block.setType(Material.AIR);

        BlockData created = Bukkit.createBlockData("campfire[lit=false]");
        require(created instanceof Campfire && !((Lightable) created).isLit(), "createBlockData");
        require(((Lightable) Material.CAMPFIRE.createBlockData()).isLit(), "defaults come from the registry");
    }

    /** Registers two recipes with named, tagged results and opens a crafting table for the client to use. */
    private void prepareCrafting(Player player) {
        ItemStack radish = named(new ItemStack(Material.SWEET_BERRIES, 2), "Vigorous radish", "radish");
        ShapelessRecipe shapeless = new ShapelessRecipe(new NamespacedKey(this, "radish"), radish);
        shapeless.addIngredient(Material.BEETROOT);
        shapeless.addIngredient(Material.SUGAR);
        require(Bukkit.addRecipe(shapeless), "addRecipe(shapeless)");

        ShapedRecipe shaped = new ShapedRecipe(new NamespacedKey(this, "whistle"),
            named(new ItemStack(Material.GOAT_HORN), "Whistle", "whistle"));
        shaped.shape("II", "IE");
        shaped.setIngredient('I', Material.IRON_INGOT);
        shaped.setIngredient('E', Material.EMERALD);
        require(Bukkit.addRecipe(shaped), "addRecipe(shaped)");

        PlayerInventory inventory = player.getInventory();
        inventory.clear();
        inventory.setItem(0, new ItemStack(Material.BEETROOT));
        inventory.setItem(1, new ItemStack(Material.SUGAR));
        inventory.setItem(2, new ItemStack(Material.IRON_INGOT, 3));
        inventory.setItem(3, new ItemStack(Material.EMERALD));
        Block table = player.getLocation().getBlock().getRelative(BlockFace.SOUTH, 3);
        table.setType(Material.CRAFTING_TABLE);
        require(player.openWorkbench(table.getLocation(), true) != null, "openWorkbench");
    }

    private ItemStack named(ItemStack stack, String name, String kind) {
        ItemMeta meta = stack.getItemMeta();
        meta.setDisplayName(name);
        meta.setLore(List.of("probe"));
        meta.getPersistentDataContainer().set(new NamespacedKey(this, "kind"), PersistentDataType.STRING, kind);
        stack.setItemMeta(meta);
        return stack;
    }

    private void checkStack(ItemStack crafted, String kind) {
        Material type = kind.equals("radish") ? Material.SWEET_BERRIES : Material.GOAT_HORN;
        String name = kind.equals("radish") ? "Vigorous radish" : "Whistle";
        require(crafted != null && crafted.getType() == type, "crafted " + crafted);
        ItemMeta meta = crafted.getItemMeta();
        require(name.equals(meta.getDisplayName()), "crafted name " + meta.getDisplayName());
        require(kind.equals(meta.getPersistentDataContainer().get(
            new NamespacedKey(this, "kind"), PersistentDataType.STRING)), "crafted persistent data");
        require(kind.equals("whistle") || crafted.getAmount() == 2, "crafted amount " + crafted.getAmount());
    }

    private void checkTaken(Player player, String kind) {
        Material type = kind.equals("radish") ? Material.SWEET_BERRIES : Material.GOAT_HORN;
        for (ItemStack stack : player.getInventory().getContents()) {
            if (stack != null && stack.getType() == type) {
                checkStack(stack, kind);
                return;
            }
        }
        throw new IllegalStateException("no " + type + " in the inventory after the shift-click");
    }
}
