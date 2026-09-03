package org.bukkit;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import org.bukkit.entity.SpawnCategory;
import java.util.logging.Logger;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.ServicesManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.scheduler.BukkitScheduler;

/** What a plugin asks the server for. */
public interface Server {
    /** Returns the datapacks discovered by the active resource reload. */
    default io.papermc.paper.datapack.DatapackManager getDatapackManager() {
        return foton.FotonDatapackManager.INSTANCE;
    }
    /** Compatibility view for plugins that inspect Spigot's server config. */
    class Spigot {
        public org.bukkit.configuration.file.YamlConfiguration getConfig() {
            return new org.bukkit.configuration.file.YamlConfiguration();
        }
        public org.bukkit.configuration.file.YamlConfiguration getPaperConfig() {
            return getConfig();
        }
        public org.bukkit.configuration.file.YamlConfiguration getSpigotConfig() {
            return getConfig();
        }
    }

    default Spigot spigot() { return new Spigot(); }

    PluginManager getPluginManager();
    default org.bukkit.inventory.ItemFactory getItemFactory() { return Bukkit.getItemFactory(); }
    /** Access to compatibility conversions and other low-level server values. */
    default UnsafeValues getUnsafe() { return Bukkit.getUnsafe(); }
    default org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder owner, int size, String title) {
        return new foton.FotonCustomInventory(owner, size, title);
    }
    default org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder owner, int size) {
        return createInventory(owner, size, "");
    }
    default org.bukkit.block.data.BlockData createBlockData(String data) { return Bukkit.createBlockData(data); }
    default org.bukkit.command.PluginCommand getPluginCommand(String name) { return null; }

    Messenger getMessenger();

    BukkitScheduler getScheduler();

    ServicesManager getServicesManager();

    boolean isPrimaryThread();

    /** Steel has one global tick thread; it is the Bukkit primary thread. */
    default boolean isGlobalTickThread() {
        return isPrimaryThread();
    }

    /** Saves all connected players asynchronously. */
    default void savePlayers() { }

    /** Requests a graceful server shutdown. */
    default boolean isStopping() { return Bukkit.isStopping(); }

    default void shutdown() {
        foton.Native.shutdown();
    }

    /**
     * Region ownership collapses to the single serialized tick in Steel.
     * A location without a world cannot belong to any region.
     */
    default boolean isOwnedByCurrentRegion(Location location) {
        return location != null && location.getWorld() != null && isPrimaryThread();
    }

    /** Region ownership for a live entity on Steel's serialized tick. */
    default boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity entity) {
        return entity != null && entity.getWorld() != null && isPrimaryThread();
    }
    default boolean isOwnedByCurrentRegion(World world, int chunkX, int chunkZ) {
        return world != null && isPrimaryThread();
    }

    Collection<? extends Player> getOnlinePlayers();

    Player getPlayer(String name);

    Player getPlayer(UUID id);

    Player getPlayerExact(String name);
    default java.util.List<Player> matchPlayer(String name) {
        if (name == null || name.isEmpty()) return java.util.Collections.emptyList();
        String wanted = name.toLowerCase(java.util.Locale.ROOT);
        java.util.ArrayList<Player> result = new java.util.ArrayList<>();
        for (Player player : getOnlinePlayers()) {
            if (player.getName().toLowerCase(java.util.Locale.ROOT).contains(wanted)) result.add(player);
        }
        return result;
    }

    String getName();

    String getVersion();

    String getBukkitVersion();
    /** Average tick duration in milliseconds, measured by the server. */
    default double getAverageTickTime() { return foton.Native.serverAverageTickTime(); }

    default String getMotd() { return ""; }
    default Warning.WarningState getWarningState() { return Warning.WarningState.DEFAULT; }

    default int getPort() { return Bukkit.getPort(); }

    default String getMinecraftVersion() { return getBukkitVersion(); }
    default org.bukkit.inventory.Recipe getRecipe(NamespacedKey key) { return null; }
    default boolean addRecipe(org.bukkit.inventory.Recipe recipe) { return false; }
    default java.util.List<org.bukkit.inventory.Recipe> getRecipesFor(org.bukkit.inventory.ItemStack result) {
        return java.util.Collections.emptyList();
    }
    default java.util.Iterator<org.bukkit.inventory.Recipe> recipeIterator() {
        return java.util.Collections.emptyIterator();
    }

    default boolean removeRecipe(org.bukkit.NamespacedKey key) { return false; }

    boolean getOnlineMode();
    /** Returns active server configuration. Steel has no proxy-forwarding mode, so proxy online mode is false. */
    default io.papermc.paper.configuration.ServerConfiguration getServerConfig() {
        return new io.papermc.paper.configuration.ServerConfiguration(false);
    }

    int getMaxPlayers();
    default String getIp() { return ""; }
    default boolean getAllowFlight() { return false; }
    default GameMode getDefaultGameMode() { return GameMode.SURVIVAL; }
    default void banIP(String address) { if (address != null) getBanList(BanList.Type.IP).addBan(address, null, null, "CONSOLE"); }
    default int getViewDistance() { return 10; }
    /** The server simulation distance in chunks. */
    default int getSimulationDistance() { return 10; }
    /** Vanilla's maximum world border diameter in blocks. */
    default int getMaxWorldSize() { return 59_999_968; }

    Logger getLogger();

    default BanList<?> getBanList(BanList.Type type) { return null; }
    default BanList<?> getBanList(io.papermc.paper.ban.BanListType type) {
        return getBanList(type == io.papermc.paper.ban.BanListType.IP ? BanList.Type.IP : BanList.Type.NAME);
    }
    default java.util.Set<OfflinePlayer> getBannedPlayers() { return Bukkit.getBannedPlayers(); }
    /** Returns players currently permitted by the server whitelist. */
    default java.util.Set<OfflinePlayer> getWhitelistedPlayers() { return java.util.Collections.emptySet(); }
    default java.io.File getPluginsFolder() { return new java.io.File(System.getProperty("foton.plugins-directory", "plugins")); }
    default java.util.Set<String> getIPBans() {
        BanList<?> list = getBanList(BanList.Type.IP);
        if (list == null) return java.util.Collections.emptySet();
        java.util.LinkedHashSet<String> result = new java.util.LinkedHashSet<>();
        for (BanEntry<?> entry : list.getBanEntries()) if (entry.getTarget() instanceof String value) result.add(value);
        return java.util.Collections.unmodifiableSet(result);
    }
    default java.util.List<org.bukkit.entity.Entity> selectEntities(CommandSender sender, String selector) {
        return Bukkit.selectEntities(sender, selector);
    }
    default org.bukkit.profile.PlayerProfile createPlayerProfile(UUID uniqueId) {
        return new foton.FotonPlayerProfile(uniqueId, null);
    }
    default org.bukkit.profile.PlayerProfile createPlayerProfile(UUID uniqueId, String name) {
        return new foton.FotonPlayerProfile(uniqueId, name);
    }

    ConsoleCommandSender getConsoleSender();

    List<World> getWorlds();

    /** Returns whether every currently loaded world generates structures. */
    default boolean getGenerateStructures() {
        java.util.List<World> worlds = getWorlds();
        return worlds.isEmpty() || worlds.stream().allMatch(World::canGenerateStructures);
    }

    /** Returns the world type of the primary loaded world. */
    default String getWorldType() {
        java.util.List<World> worlds = getWorlds();
        return worlds.isEmpty() ? WorldType.NORMAL.name() : worlds.get(0).getWorldType().name();
    }

    /** Server-wide natural spawning defaults, taken from the primary world. */
    default int getMonsterSpawnLimit() { return primaryWorldSpawnLimit(SpawnCategory.MONSTER, 70); }
    default int getAnimalSpawnLimit() { return primaryWorldSpawnLimit(SpawnCategory.ANIMAL, 15); }
    default int getWaterAnimalSpawnLimit() { return primaryWorldSpawnLimit(SpawnCategory.WATER_ANIMAL, 5); }
    default int getAmbientSpawnLimit() { return primaryWorldSpawnLimit(SpawnCategory.AMBIENT, 15); }
    default int getTicksPerMonsterSpawns() { return 20; }
    default int getTicksPerAnimalSpawns() { return 400; }
    default int getTicksPerWaterSpawns() { return 400; }
    default int getTicksPerAmbientSpawns() { return 400; }
    private int primaryWorldSpawnLimit(SpawnCategory category, int fallback) {
        List<World> worlds = getWorlds();
        if (worlds == null || worlds.isEmpty() || worlds.get(0) == null) return fallback;
        World world = worlds.get(0);
        return switch (category) {
            case MONSTER -> world.getMonsterSpawnLimit();
            case ANIMAL -> world.getAnimalSpawnLimit();
            case WATER_ANIMAL -> world.getWaterAnimalSpawnLimit();
            case AMBIENT -> world.getAmbientSpawnLimit();
            default -> fallback;
        };
    }

    /** Directory containing the server's world directories. */
    default java.io.File getWorldContainer() {
        for (World world : getWorlds()) {
            if (world != null && world.getWorldFolder() != null) {
                java.io.File parent = world.getWorldFolder().getAbsoluteFile().getParentFile();
                if (parent != null) return parent;
            }
        }
        return new java.io.File(".").getAbsoluteFile();
    }

    org.bukkit.scoreboard.ScoreboardManager getScoreboardManager();

    World getWorld(String name);

    default org.bukkit.entity.Entity getEntity(java.util.UUID uuid) {
        if (uuid == null) return null;
        for (World world : getWorlds()) {
            for (org.bukkit.entity.Entity entity : world.getEntities()) {
                if (uuid.equals(entity.getUniqueId())) return entity;
            }
        }
        return null;
    }

    int broadcastMessage(String message);

    void sendPluginMessage(org.bukkit.plugin.Plugin source, String channel, byte[] message);

    boolean dispatchCommand(CommandSender sender, String command);
    default org.bukkit.command.CommandMap getCommandMap() { return null; }

    OfflinePlayer getOfflinePlayer(String name);

    OfflinePlayer getOfflinePlayer(UUID id);
    default OfflinePlayer[] getOfflinePlayers() { return new OfflinePlayer[0]; }

    org.bukkit.boss.BossBar createBossBar(
        String title,
        org.bukkit.boss.BarColor color,
        org.bukkit.boss.BarStyle style,
        org.bukkit.boss.BarFlag... flags);

    io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler getGlobalRegionScheduler();

    io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler();

    io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler();
}
