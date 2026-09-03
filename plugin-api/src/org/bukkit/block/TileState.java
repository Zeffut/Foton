package org.bukkit.block;

import org.bukkit.persistence.PersistentDataContainer;

/** Snapshot of a block backed by a vanilla block entity. */
public interface TileState extends BlockState {
    @Override
    PersistentDataContainer getPersistentDataContainer();
}
