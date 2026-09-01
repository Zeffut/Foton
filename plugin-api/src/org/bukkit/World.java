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

    default java.util.concurrent.CompletableFuture<Chunk> getChunkAtAsync(int x, int z, boolean urgent) {
        return java.util.concurrent.CompletableFuture.completedFuture(getChunkAt(x, z));
    }

    default java.util.concurrent.CompletableFuture<Chunk> getChunkAtAsync(
            int x, int z, boolean generate, boolean urgent) {
        return getChunkAtAsync(x, z, urgent);
    }

    interface ChunkLoadCallback {
        void onLoad(Chunk chunk);
    }

    default void getChunkAtAsync(int x, int z, ChunkLoadCallback callback) {
        if (callback == null) return;
        getChunkAtAsync(x, z, true).thenAccept(callback::onLoad);
    }

    Chunk getChunkAt(Location location);

    default org.bukkit.entity.Item dropItem(Location location, org.bukkit.inventory.ItemStack item) { return null; }

    default boolean isChunkLoaded(int x, int z) { return false; }
    default java.io.File getWorldFolder() { return null; }
    default boolean isAutoSave() { return true; }
    default void setAutoSave(boolean value) { }
    default void save() { }

    default Chunk[] getLoadedChunks() {
        return new Chunk[0];
    }

    long getTime();

    long getFullTime();

    int getMinHeight();
    int getMaxHeight();

    java.util.List<org.bukkit.entity.Player> getPlayers();
    java.util.List<org.bukkit.entity.Entity> getEntities();

    default java.util.Collection<org.bukkit.entity.Entity> getEntitiesByClasses(Class<?>... classes) {
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        if (classes == null) return result;
        for (org.bukkit.entity.Entity entity : getEntities()) {
            for (Class<?> type : classes) {
                if (type != null && type.isInstance(entity)) { result.add(entity); break; }
            }
        }
        return result;
    }

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
