package org.bukkit.block.data.type;

import org.bukkit.block.data.Bisected;
import org.bukkit.block.data.Directional;
import org.bukkit.block.data.Openable;
import org.bukkit.block.data.Powerable;

/** Door block data contract. */
public interface Door extends Bisected, Directional, Openable, Powerable {
    enum Hinge { LEFT, RIGHT }
    Hinge getHinge();
    void setHinge(Hinge hinge);
}
