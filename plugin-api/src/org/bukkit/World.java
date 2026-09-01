package org.bukkit;

import java.util.UUID;

/** One world on the server. */
public interface World {
    String getName();

    UUID getUID();

    NamespacedKey getKey();

    Location getSpawnLocation();

    org.bukkit.block.Block getBlockAt(int x, int y, int z);

    org.bukkit.block.Block getBlockAt(Location location);

    Chunk getChunkAt(int x, int z);

    Chunk getChunkAt(Location location);

    long getTime();

    long getFullTime();

    int getMinHeight();
    int getMaxHeight();

    Environment getEnvironment();

    enum Environment {
        NETHER(-1),
        NORMAL(0),
        THE_END(1),
        CUSTOM(Integer.MIN_VALUE);

        private final int id;

        Environment(int id) {
            this.id = id;
        }

        public int getId() {
            return id;
        }
    }
}
