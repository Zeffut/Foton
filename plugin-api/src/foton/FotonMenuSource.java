package foton;

import org.bukkit.Material;
import org.bukkit.block.Block;
import org.bukkit.entity.Entity;
import org.bukkit.event.inventory.InventoryType;
import org.bukkit.inventory.InventoryHolder;

/** What a player's open menu was opened on, and what Bukkit makes of it.
 *
 * <p>Bukkit names an inventory by what is behind it: the holder of a
 * furnace's screen is the furnace, of a chest boat's the boat, and 27 slots
 * in a barrel are a {@code BARREL} while the same 27 in a chest are a
 * {@code CHEST}. A plugin tells containers apart by exactly that. */
final class FotonMenuSource {
    private FotonMenuSource() {}

    /** The block or the entity, one of them set; null for a menu opened on nothing. */
    record Source(Block block, Entity entity) {}

    static Source of(String owner) {
        String text = Native.openMenuSource(owner);
        if (text == null) return null;
        String[] parts = text.split(" ");
        try {
            if (parts.length == 2 && parts[0].equals("entity")) {
                return new Source(null, FotonEntity.handle(Native.parse(parts[1])));
            }
            if (parts.length == 4 && parts[0].equals("block")) {
                FotonPlayer player = new FotonPlayer(java.util.UUID.fromString(owner));
                return new Source(player.getWorld().getBlockAt(Integer.parseInt(parts[1]),
                    Integer.parseInt(parts[2]), Integer.parseInt(parts[3])), null);
            }
        } catch (RuntimeException unreadable) {
            return null;
        }
        return null;
    }

    /** Bukkit's holder: the block's state or the entity when either holds an
     * inventory; the player for an ender chest, whose contents are theirs. */
    static InventoryHolder holder(String owner) {
        Source source = of(owner);
        if (source == null) return null;
        if (source.entity() != null) return source.entity() instanceof InventoryHolder holder ? holder : null;
        if (source.block().getType() == Material.ENDER_CHEST) {
            return new FotonPlayer(java.util.UUID.fromString(owner));
        }
        return source.block().getState() instanceof InventoryHolder holder ? holder : null;
    }

    /** Bukkit's type for the open menu, or null when only its size can say. */
    static InventoryType type(String owner) {
        String menu = Native.openMenuType(owner);
        if (menu == null) return null;
        return switch (menu) {
            case "minecraft:anvil" -> InventoryType.ANVIL;
            case "minecraft:grindstone" -> InventoryType.GRINDSTONE;
            case "minecraft:smithing" -> InventoryType.SMITHING;
            case "minecraft:enchantment" -> InventoryType.ENCHANTING;
            case "minecraft:brewing_stand" -> InventoryType.BREWING;
            case "minecraft:cartography_table" -> InventoryType.CARTOGRAPHY;
            case "minecraft:loom" -> InventoryType.LOOM;
            case "minecraft:stonecutter" -> InventoryType.STONECUTTER;
            case "minecraft:merchant" -> InventoryType.MERCHANT;
            case "minecraft:beacon" -> InventoryType.BEACON;
            case "minecraft:hopper" -> InventoryType.HOPPER;
            case "minecraft:crafter_3x3" -> InventoryType.CRAFTER;
            case "minecraft:shulker_box" -> InventoryType.SHULKER_BOX;
            case "minecraft:lectern" -> InventoryType.LECTERN;
            case "minecraft:crafting" -> InventoryType.WORKBENCH;
            case "minecraft:generic_3x3" -> blockIs(owner, Material.DROPPER) ? InventoryType.DROPPER : InventoryType.DISPENSER;
            default -> menu.startsWith("minecraft:generic_9x") ? chestLike(owner) : null;
        };
    }

    private static boolean blockIs(String owner, Material material) {
        Source source = of(owner);
        return source != null && source.block() != null && source.block().getType() == material;
    }

    /** A 9-wide grid is a chest unless a barrel or an ender chest holds it. */
    private static InventoryType chestLike(String owner) {
        Source source = of(owner);
        if (source == null) return null;
        if (source.block() == null) return InventoryType.CHEST;
        return switch (source.block().getType()) {
            case BARREL -> InventoryType.BARREL;
            case ENDER_CHEST -> InventoryType.ENDER_CHEST;
            default -> InventoryType.CHEST;
        };
    }
}
