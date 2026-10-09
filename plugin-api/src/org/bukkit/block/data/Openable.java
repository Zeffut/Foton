package org.bukkit.block.data;

/** Block data that opens and closes: doors, trapdoors and fence gates. */
public interface Openable extends BlockData {
    boolean isOpen();
    void setOpen(boolean open);
}
