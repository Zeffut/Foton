package org.bukkit.inventory;

/** Six-slot inventory of a chiseled bookshelf. */
public interface ChiseledBookshelfInventory extends Inventory {
    @Override default int getSize() { return 6; }
}
