package org.bukkit.event.inventory;

/** Vanilla container kinds exposed by Bukkit. */
public enum InventoryType {
    CHEST(27), DISPENSER(9), DROPPER(9), FURNACE(3), WORKBENCH(10), CRAFTING(5),
    ENCHANTING(2), BREWING(5), PLAYER(41), CREATIVE(45), MERCHANT(3), ENDER_CHEST(27),
    ANVIL(3), SMITHING(4), BEACON(1), HOPPER(5), SHULKER_BOX(27), BARREL(27),
    BLAST_FURNACE(3), LECTERN(1), SMOKER(3), LOOM(4), CARTOGRAPHY(3), GRINDSTONE(3),
    STONECUTTER(2), CRAFTER(10), HORSE(2), DONKEY(2), MULE(2), LLAMA(2), CAMEL(2), CHEST_HORSE(17),
    CHEST_DONKEY(17), CHEST_MULE(17), CHEST_LLAMA(17), JUKEBOX(1), GENERIC_9X1(9),
    GENERIC_9X2(18), GENERIC_9X3(27), GENERIC_9X4(36), GENERIC_9X5(45), GENERIC_9X6(54),
    UNKNOWN(0);

    public enum SlotType { CONTAINER, QUICKBAR, ARMOR, RESULT, FUEL, OUTSIDE, CRAFTING, UNKNOWN }

    private final int size;
    InventoryType(int size) { this.size = size; }
    public int getDefaultSize() { return size; }
}
