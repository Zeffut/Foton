package org.bukkit;

import java.util.UUID;

/** One world on the server. */
public interface World extends org.bukkit.generator.WorldInfo, RegionAccessor, org.bukkit.metadata.Metadatable {
    default void spawnParticle(org.bukkit.Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, double extra) {
        if (particle == null || location == null || location.getWorld() != this) return;
        foton.Native.spawnParticle(getName(), "minecraft:" + particle.name().toLowerCase(java.util.Locale.ROOT),
            location.getX(), location.getY(), location.getZ(), count, offsetX, offsetY, offsetZ, extra);
    }
    String getName();
    default long getSeed() { return 0L; }
    default double getCoordinateScale() { return 1.0D; }
    default WorldType getWorldType() { return WorldType.NORMAL; }
    default Difficulty getDifficulty() { return Difficulty.NORMAL; }
    default void setDifficulty(Difficulty difficulty) { }
    default boolean hasBonusChest() { return foton.Native.worldHasBonusChest(getName()); }
    default boolean getPVP() { return true; }
    default boolean getAllowMonsters() { return true; }
    default boolean getAllowAnimals() { return true; }
    default void setSpawnFlags(boolean allowMonsters, boolean allowAnimals) { }
    default void setPVP(boolean enabled) { }
    default boolean getKeepSpawnInMemory() { return true; }
    default void setKeepSpawnInMemory(boolean keep) { }
    /** Whether the vanilla spawn area is kept loaded in memory. */
    default void setSpawnLimit(org.bukkit.entity.SpawnCategory category, int limit) { }
    default void setTicksPerSpawns(org.bukkit.entity.SpawnCategory category, int ticks) { }
    default int getMonsterSpawnLimit() { return 70; }
    default int getAnimalSpawnLimit() { return 15; }
    default int getWaterAnimalSpawnLimit() { return 5; }
    default int getAmbientSpawnLimit() { return 15; }
    /** Whether this world generator places structures. */
    default boolean canGenerateStructures() { return true; }
    /** Returns the Bukkit generator when this world was created by one. Steel currently has no Bukkit generator registry, so vanilla worlds return null. */
    default org.bukkit.generator.ChunkGenerator getGenerator() { return null; }

    UUID getUID();

    NamespacedKey getKey();

    Location getSpawnLocation();

    /** Sets the world spawn position, matching Bukkit semantics. */
    default boolean setSpawnLocation(Location location) { return false; }

    org.bukkit.block.Block getBlockAt(int x, int y, int z);

    org.bukkit.block.Block getBlockAt(Location location);

    Chunk getChunkAt(int x, int z);
    default Chunk getChunkAt(int x, int z, boolean generate) { return getChunkAt(x, z); }
    default void playEffect(Location location, Effect effect, int data) { }
    default void playEffect(Location location, Effect effect, Object data) {
        if (data instanceof Number number) playEffect(location, effect, number.intValue());
    }
    default boolean addPluginChunkTicket(int x, int z, org.bukkit.plugin.Plugin plugin) { return false; }
    default boolean removePluginChunkTicket(int x, int z, org.bukkit.plugin.Plugin plugin) { return false; }

    default java.util.concurrent.CompletableFuture<Chunk> getChunkAtAsync(int x, int z) {
        return getChunkAtAsync(x, z, true);
    }

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
    /** Creates a vanilla-style destructive explosion at the location. */
    default boolean createExplosion(Location location, float power) {
        if (location == null || location.getWorld() != this || power < 0.0f
                || !Double.isFinite(location.getX()) || !Double.isFinite(location.getY()) || !Double.isFinite(location.getZ())) return false;
        return foton.Native.createExplosion(getName(), location.getX(), location.getY(), location.getZ(), power);
    }
    default boolean createExplosion(Location location, float power, boolean setFire) {
        if (location == null || location.getWorld() != this || power < 0.0f
                || !Double.isFinite(location.getX()) || !Double.isFinite(location.getY()) || !Double.isFinite(location.getZ())) return false;
        return foton.Native.createExplosionAdvanced(getName(), location.getX(), location.getY(), location.getZ(), power, setFire, true);
    }
    default boolean createExplosion(Location location, float power, boolean setFire, boolean breakBlocks) {
        if (location == null || location.getWorld() != this || power < 0.0f) return false;
        return foton.Native.createExplosionAdvanced(getName(), location.getX(), location.getY(), location.getZ(), power, setFire, breakBlocks);
    }
    default boolean createExplosion(double x, double y, double z, float power) {
        if (!Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z)) return false;
        return createExplosion(new Location(this, x, y, z), power);
    }
    default boolean createExplosion(double x, double y, double z, float power, boolean setFire) {
        if (!Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z)) return false;
        return createExplosion(new Location(this, x, y, z), power, setFire);
    }
    default boolean createExplosion(double x, double y, double z, float power, boolean setFire, boolean breakBlocks) {
        if (!Double.isFinite(x) || !Double.isFinite(y) || !Double.isFinite(z)) return false;
        return createExplosion(new Location(this, x, y, z), power, setFire, breakBlocks);
    }
    default org.bukkit.entity.Item dropItemNaturally(Location location, org.bukkit.inventory.ItemStack item) {
        return dropItem(location, item);
    }
    default org.bukkit.entity.Entity spawnEntity(Location location, org.bukkit.entity.EntityType type) { return null; }
    default org.bukkit.entity.LightningStrike strikeLightning(Location location) { return null; }
    default org.bukkit.entity.LightningStrike strikeLightningEffect(Location location) { return null; }
    default <T extends org.bukkit.entity.Entity> T spawn(Location location, Class<T> clazz) { return null; }
    default <T extends org.bukkit.entity.Entity> T spawn(Location location, Class<T> clazz,
            java.util.function.Consumer<? super T> function) {
        T entity = spawn(location, clazz);
        if (entity != null && function != null) function.accept(entity);
        return entity;
    }
    default boolean generateTree(Location location, TreeType type) { return false; }

    default boolean isChunkLoaded(int x, int z) { return false; }
    default boolean isChunkLoaded(Chunk chunk) { return chunk != null && isChunkLoaded(chunk.getX(), chunk.getZ()); }
    default boolean isChunkGenerated(int x, int z) { return isChunkLoaded(x, z); }
    default String getGameRuleValue(String rule) { return null; }
    default <T> boolean setGameRule(GameRule<T> rule, T value) {
        return rule != null && value != null && foton.Native.setWorldGameRule(getName(), rule.getName(), String.valueOf(value));
    }
    default <T> T getGameRuleDefault(GameRule<T> rule) {
        return rule == null ? null : rule.parse(foton.Native.worldGameRuleDefault(getName(), rule.getName()));
    }
    default <T> T getGameRuleValue(GameRule<T> rule) {
        return rule == null ? null : rule.parse(getGameRuleValue(rule.getName()));
    }
    default String[] getGameRules() {
        return java.util.Arrays.stream(GameRule.values()).map(GameRule::getName).toArray(String[]::new);
    }
    default boolean hasStorm() { return false; }
    default boolean isClearWeather() { return !hasStorm() && !isThundering(); }
    default void setStorm(boolean storm) { }
    default int getWeatherDuration() { return foton.Native.worldWeatherDuration(getName()); }
    default void setWeatherDuration(int duration) { foton.Native.setWorldWeatherDuration(getName(), duration); }
    default int getThunderDuration() { return foton.Native.worldThunderDuration(getName()); }
    default void setThunderDuration(int duration) { foton.Native.setWorldThunderDuration(getName(), duration); }
    default boolean isThundering() { return false; }
    default void setThundering(boolean thundering) { }
    default java.io.File getWorldFolder() { return null; }
    default boolean isAutoSave() { return true; }
    default void setAutoSave(boolean value) { }
    default void save() { }

    default Chunk[] getLoadedChunks() {
        return new Chunk[0];
    }

    long getTime();

    /** Sets the world day time, in ticks. */
    default void setTime(long time) { foton.Native.setWorldTime(getName(), time); }

    long getFullTime();

    int getMinHeight();
    int getMaxHeight();

    default WorldBorder getWorldBorder() { return null; }
    default int getLogicalHeight() { return getMaxHeight() - getMinHeight(); }
    default int getSeaLevel() { return 63; }
    default int getHighestBlockYAt(int x, int z) {
        for (int y = getMaxHeight() - 1; y >= getMinHeight(); y--) {
            if (!getBlockAt(x, y, z).isEmpty()) return y;
        }
        return getMinHeight();
    }
    default int getHighestBlockYAt(Location location) {
        if (location == null) return getMinHeight();
        return getHighestBlockYAt(location.getBlockX(), location.getBlockZ());
    }

    java.util.List<org.bukkit.entity.Player> getPlayers();
    java.util.List<org.bukkit.entity.Entity> getEntities();
    default java.util.List<org.bukkit.entity.LivingEntity> getLivingEntities() {
        java.util.ArrayList<org.bukkit.entity.LivingEntity> result = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity entity : getEntities())
            if (entity instanceof org.bukkit.entity.LivingEntity living) result.add(living);
        return java.util.Collections.unmodifiableList(result);
    }

    default java.util.Collection<org.bukkit.entity.Entity> getNearbyEntities(
            Location location, double x, double y, double z,
            java.util.function.Predicate<org.bukkit.entity.Entity> filter) {
        java.util.Collection<org.bukkit.entity.Entity> entities = getNearbyEntities(location, x, y, z);
        if (filter == null) return entities;
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity entity : entities) if (filter.test(entity)) result.add(entity);
        return result;
    }

    default java.util.Collection<org.bukkit.entity.Entity> getNearbyEntities(
            Location location, double x, double y, double z) {
        if (location == null || location.getWorld() != this || x < 0 || y < 0 || z < 0) {
            return java.util.Collections.emptyList();
        }
        java.util.ArrayList<org.bukkit.entity.Entity> result = new java.util.ArrayList<>();
        for (org.bukkit.entity.Entity entity : getEntities()) {
            Location at = entity.getLocation();
            if (at != null && Math.abs(at.getX() - location.getX()) <= x
                    && Math.abs(at.getY() - location.getY()) <= y
                    && Math.abs(at.getZ() - location.getZ()) <= z) result.add(entity);
        }
        return result;
    }

    default void playSound(Location location, Sound sound, float volume, float pitch) {
        if (location != null) playSound(location, sound == null ? null : sound.getKey(), volume, pitch);
    }
    default void playSound(org.bukkit.entity.Entity entity, Sound sound, float volume, float pitch) {
        if (entity != null) playSound(entity.getLocation(), sound, volume, pitch);
    }
    default void spawnParticle(Particle particle, Location location, int count) {
        spawnParticle(particle, location, count, 0, 0, 0, 0);
    }
    default void spawnParticle(Particle particle, Location location, int count, Object data) {
        spawnParticle(particle, location, count, 0, 0, 0, 0);
    }

    default void spawnParticle(Particle particle, Location location, int count,
            double offsetX, double offsetY, double offsetZ, Object data) {
        spawnParticle(particle, location, count, offsetX, offsetY, offsetZ, 0);
    }
    default void playSound(Location location, String sound, float volume, float pitch) { }
    default void playSound(Location location, Sound sound, SoundCategory category, float volume, float pitch) {
        if (location != null) playSound(location, sound == null ? null : sound.getKey(), category, volume, pitch);
    }
    default void playSound(Location location, String sound, SoundCategory category, float volume, float pitch) { }

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
