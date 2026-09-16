package io.papermc.paper.block;

/** Paper composition shared by lockable named tile states. */
public interface LockableTileState extends org.bukkit.block.TileState,
        org.bukkit.block.Lockable, org.bukkit.Nameable { }
