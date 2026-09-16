package io.papermc.paper.block;

import org.bukkit.inventory.Inventory;
import org.jetbrains.annotations.NotNull;

/** Tile state exposing both its placed and detached inventory views. */
public interface TileStateInventoryHolder extends org.bukkit.block.TileState,
        org.bukkit.inventory.BlockInventoryHolder {
    @Override @NotNull Inventory getInventory();
    @NotNull Inventory getSnapshotInventory();
}
