package org.bukkit.block;

import org.bukkit.inventory.ItemStack;
import org.jetbrains.annotations.NotNull;

/** Tile state whose vanilla lock key can be inspected and changed. */
public interface Lockable {
    boolean isLocked();
    @NotNull String getLock();
    void setLock(@NotNull String key);
    void setLockItem(@NotNull ItemStack key);
}
