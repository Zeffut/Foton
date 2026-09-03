package org.bukkit.generator;

/** Stable world identity exposed to generators and world environments. */
public interface WorldInfo {
    String getName();

    default int getMinHeight() { return 0; }
    default int getMaxHeight() { return 256; }
}
