import java.util.List;
import org.bukkit.Material;
import org.bukkit.inventory.ItemStack;
import org.bukkit.inventory.meta.ItemMeta;

/** Material, ItemStack, and the string one inventory slot crosses as. */
final class Items {
    private Items() {}

    static void check() {
        materials();
        stacks();
        slots();
    }

    private static void materials() {
        // The enum is generated from the same registry the server is built
        // from, so a name Foton knows is a name a plugin can reach.
        Checks.expect(Material.DIAMOND_SWORD != null, "a common item exists");
        Checks.expect(Material.AIR.isAir(), "air is air");
        Checks.expect(Material.STONE.isBlock() && Material.STONE.isItem(),
            "stone is both a block and an item");

        // A wall sign is a block with no item. A plugin that put one in an
        // inventory would have made a stack of nothing, and isItem is how it
        // finds out.
        Checks.expect(Material.OAK_WALL_SIGN.isBlock(), "a wall sign is a block");
        Checks.expect(!Material.OAK_WALL_SIGN.isItem(), "a wall sign is not an item");

        Checks.same(Material.DIAMOND_SWORD.getMaxStackSize(), 1, "a sword does not stack");
        Checks.same(Material.STONE.getMaxStackSize(), 64, "stone stacks to 64");

        // Three spellings, because that is what a config file contains.
        Checks.same(Material.matchMaterial("DIAMOND_SWORD"), Material.DIAMOND_SWORD, "by name");
        Checks.same(Material.matchMaterial("diamond_sword"), Material.DIAMOND_SWORD, "by key");
        Checks.same(Material.matchMaterial("minecraft:diamond_sword"), Material.DIAMOND_SWORD,
            "by namespaced key");
        Checks.same(Material.matchMaterial("not_a_thing"), null,
            "an unknown name answers null rather than throwing");
        Checks.same(Material.DIAMOND_SWORD.getKey().toString(), "minecraft:diamond_sword",
            "the key round-trips");
    }

    private static void stacks() {
        ItemStack sword = new ItemStack(Material.DIAMOND_SWORD);
        Checks.same(sword.getAmount(), 1, "a stack defaults to one");

        ItemStack stone = new ItemStack(Material.STONE, 32);
        Checks.same(stone.getAmount(), 32, "a stack keeps its count");

        // The meta comes back as a copy, which is Bukkit's trap: a plugin
        // mutates what it got and must call setItemMeta or nothing happens.
        // Behaving differently would make plugins written against it wrong.
        ItemMeta meta = sword.getItemMeta();
        meta.setDisplayName("Excalibur");
        Checks.expect(!sword.getItemMeta().hasDisplayName(),
            "mutating the returned meta must not reach the item");
        sword.setItemMeta(meta);
        Checks.same(sword.getItemMeta().getDisplayName(), "Excalibur",
            "setItemMeta is what makes it stick");

        meta.setLore(List.of("one", "two"));
        sword.setItemMeta(meta);
        Checks.same(sword.getItemMeta().getLore(), List.of("one", "two"), "lore sticks");

        // Setting the type to air empties the stack, which is what a plugin
        // clearing a slot relies on.
        ItemStack cleared = new ItemStack(Material.STONE, 32);
        cleared.setType(Material.AIR);
        Checks.same(cleared.getAmount(), 0, "becoming air empties the stack");

        Checks.expect(new ItemStack(Material.STONE, 1).isSimilar(new ItemStack(Material.STONE, 64)),
            "isSimilar ignores the count");
        Checks.expect(!new ItemStack(Material.STONE, 1).equals(new ItemStack(Material.STONE, 64)),
            "equals does not");
        Checks.expect(!sword.isSimilar(new ItemStack(Material.DIAMOND_SWORD)),
            "a named sword is not a plain one");
    }

    /** One slot crosses JNI as a string, so the string has to survive. */
    private static void slots() {
        ItemStack stone = new ItemStack(Material.STONE, 32);
        Checks.same(foton.FotonInventory.encode(stone), "minecraft:stone 32", "encoded");
        ItemStack read = foton.FotonInventory.decode("minecraft:stone 32");
        Checks.same(read.getType(), Material.STONE, "decoded type");
        Checks.same(read.getAmount(), 32, "decoded amount");

        // Empty and unreadable are different answers. An empty slot is the
        // empty string; anything that cannot be read is null, so a plugin
        // never reads a slot it could not see as an empty one.
        Checks.same(foton.FotonInventory.encode(null), "", "nothing encodes to nothing");
        Checks.same(foton.FotonInventory.encode(new ItemStack(Material.AIR)), "",
            "air encodes to nothing");
        Checks.same(foton.FotonInventory.decode(""), null, "an empty slot decodes to nothing");
        Checks.same(foton.FotonInventory.decode(null), null, "an unreadable slot is nothing");
        Checks.same(foton.FotonInventory.decode("minecraft:not_a_thing 1"), null,
            "an unknown item is nothing rather than a guess");
        Checks.same(foton.FotonInventory.decode("minecraft:stone 0"), null,
            "a count of zero is nothing");
    }
}
