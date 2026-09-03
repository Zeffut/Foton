package org.bukkit.block;

/** Command block state. */
public interface CommandBlock extends BlockState {
    String getCommand();
    void setCommand(String command);
}
