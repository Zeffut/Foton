package org.bukkit.block.data.type;
import org.bukkit.block.data.BlockData;
/** Vanilla cake bite state. */
public interface Cake extends BlockData { int getBites(); void setBites(int bites); int getMaximumBites(); }
