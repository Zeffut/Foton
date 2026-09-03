package org.bukkit.block.data.type;

import org.bukkit.block.data.Bisected;
import org.bukkit.block.data.Directional;

/** Door block data contract. */
public interface Door extends Bisected, Directional {
    enum Hinge { LEFT, RIGHT }
    default Hinge getHinge() { return Hinge.LEFT; }
    default void setHinge(Hinge hinge) { }
    default boolean isOpen() { return false; }
    default void setOpen(boolean open) { }
}
