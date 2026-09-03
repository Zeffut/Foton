package org.bukkit;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import java.util.logging.Logger;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.plugin.PluginManager;
import org.bukkit.scheduler.BukkitScheduler;

/** The static door onto the running server, which most plugins use.
 *
 * Every method here forwards to the one Server. Plugins reach for the statics
 * far more than the interface -- `Bukkit.getVersion` alone is called by
 * thirty-seven of the fifty-nine plugins surveyed -- so the two have to stay
 * in step.
 */
public final class Bukkit {
    /** Returns the server's datapack inventory. */
    public static io.papermc.paper.datapack.DatapackManager getDatapackManager() {
        return getServer().getDatapackManager();
    }
    /** Creates a command sender that forwards plain-text messages to a callback. */
    public static org.bukkit.command.CommandSender createCommandSender(
            java.util.function.Consumer<String> messageConsumer) {
        java.util.Objects.requireNonNull(messageConsumer, "messageConsumer");
        return new org.bukkit.command.CommandSender() {
            @Override public void sendMessage(String message) {
                if (message != null) messageConsumer.accept(message);
            }
            @Override public boolean hasPermission(String permission) { return true; }
            @Override public String getName() { return "CommandSender"; }
        };
    }

    private static Server server;
    private static final org.bukkit.inventory.ItemFactory ITEM_FACTORY = new org.bukkit.inventory.ItemFactory() {};

    private Bukkit() {}

    public static Server getServer() {
        return server;
    }
    public static org.bukkit.inventory.ItemFactory getItemFactory() { return ITEM_FACTORY; }
    public static <T extends Keyed> Registry<T> getRegistry(Class<T> type) {
        if (type == org.bukkit.Art.class) return (Registry<T>) Registry.ART;
        if (type == org.bukkit.attribute.Attribute.class) return (Registry<T>) Registry.ATTRIBUTE;
        if (type == org.bukkit.Material.class) return (Registry<T>) Registry.MATERIAL;
        if (type == org.bukkit.block.Biome.class) return (Registry<T>) Registry.BIOME;
        if (type == org.bukkit.enchantments.Enchantment.class) return (Registry<T>) Registry.ENCHANTMENT;
        if (type == org.bukkit.Particle.class) return (Registry<T>) Registry.PARTICLE_TYPE;
        if (type == org.bukkit.inventory.meta.trim.TrimPattern.class) return (Registry<T>) Registry.TRIM_PATTERN;
        if (type == org.bukkit.inventory.meta.trim.TrimMaterial.class) return (Registry<T>) Registry.TRIM_MATERIAL;
        throw new IllegalArgumentException("Unsupported registry type: " + type);
    }
    private static volatile boolean stopping;
    public static boolean isStopping() { return stopping; }
    public static String getMinecraftVersion() { return server == null ? "" : server.getMinecraftVersion(); }
    public static String getMotd() { return server == null ? "" : server.getMotd(); }
    public static double[] getTPS() { return foton.Native.serverTps(); }
    public static double getAverageTickTime() { return foton.Native.serverAverageTickTime(); }
    public static org.bukkit.block.data.BlockData createBlockData(String data) {
        return new org.bukkit.block.data.SimpleBlockData(data);
    }

    public static void shutdown() {
        stopping = true;
        if (server != null) server.shutdown();
    }

    public static Server.Spigot spigot() {
        return server == null ? new Server.Spigot() : server.spigot();
    }

    /** Returns the configured server IP; Steel binds the unspecified address. */
    public static String getIp() {
        return "";
    }

    /** Returns the port supplied by the host, or -1 when it was not exposed. */
    public static int getPort() {
        String value = System.getProperty("foton.server-port");
        if (value == null) return -1;
        try {
            return Integer.parseInt(value);
        } catch (NumberFormatException ignored) {
            return -1;
        }
    }

    public static boolean removeRecipe(NamespacedKey key) {
        return server != null && server.removeRecipe(key);
    }

    public static boolean addRecipe(org.bukkit.inventory.Recipe recipe) {
        return server != null && server.addRecipe(recipe);
    }

    public static void setServer(Server value) {
        if (server != null) {
            throw new UnsupportedOperationException("Cannot redefine singleton Server");
        }
        server = value;
    }

    public static PluginManager getPluginManager() {
        return server.getPluginManager();
    }

    public static org.bukkit.command.CommandMap getCommandMap() { return server.getCommandMap(); }

    public static UnsafeValues getUnsafe() {
        return new foton.FotonUnsafeValues();
    }

    public static org.bukkit.plugin.messaging.Messenger getMessenger() {
        return server.getMessenger();
    }

    public static BukkitScheduler getScheduler() {
        return server.getScheduler();
    }

    public static org.bukkit.plugin.ServicesManager getServicesManager() {
        return server.getServicesManager();
    }

    public static boolean isPrimaryThread() {
        return server.isPrimaryThread();
    }

    public static Collection<? extends Player> getOnlinePlayers() {
        return server.getOnlinePlayers();
    }

    public static Player getPlayer(String name) {
        return server.getPlayer(name);
    }

    public static Player getPlayer(UUID id) {
        return server.getPlayer(id);
    }
    public static org.bukkit.entity.Entity getEntity(UUID id) { return server.getEntity(id); }
    public static org.bukkit.entity.EntityFactory getEntityFactory() { return new foton.FotonEntityFactory(); }
    /** Steel runs entity work on the owning server thread, so a live entity is owned here. */
    public static boolean isOwnedByCurrentRegion(org.bukkit.entity.Entity entity) {
        return entity != null && entity.isValid();
    }
    public static boolean isOwnedByCurrentRegion(org.bukkit.World world, int chunkX, int chunkZ) {
        return server.isOwnedByCurrentRegion(world, chunkX, chunkZ);
    }
    public static org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size) {
        return new foton.FotonCustomInventory(holder, size, "");
    }
    public static org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size, String title) {
        return new foton.FotonCustomInventory(holder, size, title);
    }
    public static org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder, int size, net.kyori.adventure.text.Component title) {
        String plain = title == null ? "" : net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer.plainText().serialize(title);
        return new foton.FotonCustomInventory(holder, size, plain);
    }
    public static org.bukkit.inventory.Inventory createInventory(org.bukkit.inventory.InventoryHolder holder,
            org.bukkit.event.inventory.InventoryType type, String title) {
        if (type == null) throw new IllegalArgumentException("Inventory type cannot be null");
        int size = type.getDefaultSize();
        if (size < 1 || size > 54 || size % 9 != 0)
            throw new IllegalArgumentException("Inventory type is not a generic container: " + type);
        return new foton.FotonCustomInventory(holder, size, title);
    }
    public static com.destroystokyo.paper.profile.PlayerProfile createProfile(UUID id) {
        return new foton.FotonPlayerProfile(id, null);
    }
    public static com.destroystokyo.paper.profile.PlayerProfile createProfile(String name) {
        return new foton.FotonPlayerProfile(null, name);
    }
    public static org.bukkit.profile.PlayerProfile createPlayerProfile(UUID id, String name) {
        return new foton.FotonPlayerProfile(id, name);
    }
    public static org.bukkit.profile.PlayerProfile createPlayerProfile(UUID id) {
        return createPlayerProfile(id, null);
    }

    public static Player getPlayerExact(String name) {
        return server.getPlayerExact(name);
    }
    public static java.util.List<Player> matchPlayer(String name) { return server.matchPlayer(name); }

    public static String getName() {
        return server.getName();
    }

    public static String getVersion() {
        return server.getVersion();
    }

    public static String getBukkitVersion() {
        return server.getBukkitVersion();
    }

    public static boolean getOnlineMode() {
        return server.getOnlineMode();
    }

    public static int getMaxPlayers() {
        return server.getMaxPlayers();
    }

    public static Logger getLogger() {
        return server.getLogger();
    }

    public static BanList<?> getBanList(BanList.Type type) {
        return server.getBanList(type);
    }

    public static java.util.Set<OfflinePlayer> getBannedPlayers() {
        java.util.LinkedHashSet<OfflinePlayer> result = new java.util.LinkedHashSet<>();
        for (BanEntry<?> entry : server.getBanList(BanList.Type.NAME).getBanEntries()) {
            Object target = entry.getTarget();
            if (target instanceof String name) result.add(server.getOfflinePlayer(name));
        }
        return java.util.Collections.unmodifiableSet(result);
    }

    public static org.bukkit.advancement.Advancement getAdvancement(NamespacedKey key) {
        if (key == null) return null;
        String[] criteria = foton.Native.advancementCriteria(key.toString());
        return criteria == null ? null : new foton.FotonAdvancement(key, criteria);
    }

    private static final org.bukkit.help.HelpMap HELP_MAP = new foton.FotonHelpMap();

    public static org.bukkit.help.HelpMap getHelpMap() { return HELP_MAP; }

    public static ConsoleCommandSender getConsoleSender() {
        return server.getConsoleSender();
    }

    /** Current serialized server tick; Steel advances world time once per tick. */
    public static int getCurrentTick() {
        java.util.List<World> worlds = getWorlds();
        return worlds.isEmpty() ? 0 : (int) worlds.get(0).getFullTime();
    }

    public static boolean unloadWorld(World world, boolean save) { return world != null && foton.Native.unloadWorld(world.getName(), save); }

    public static List<World> getWorlds() {
        return server.getWorlds();
    }

    /** Directory containing the server world directories. */
    public static java.io.File getWorldContainer() { return server.getWorldContainer(); }

    public static org.bukkit.scoreboard.ScoreboardManager getScoreboardManager() {
        return server.getScoreboardManager();
    }

    public static World getWorld(String name) {
        return server.getWorld(name);
    }
    public static World getWorld(NamespacedKey key) { return key == null ? null : getWorld(key.toString()); }
    public static World getWorld(UUID uid) {
        if (uid == null) return null;
        for (World world : server.getWorlds()) {
            if (uid.equals(world.getUID())) return world;
        }
        return null;
    }

    public static <T extends Keyed> Tag<T> getTag(String registry, NamespacedKey key, Class<T> type) {
        return key == null || type == null ? null : new Tag<>(key, type, registry);
    }

    public static int broadcast(net.kyori.adventure.text.Component message) {
        if (message == null || server == null) return 0;
        int count = 0;
        for (Player player : server.getOnlinePlayers()) {
            if (player == null) continue;
            player.sendMessage(message);
            count++;
        }
        return count;
    }

    /** Broadcasts a legacy message only to players with the given permission. */
    public static int broadcast(String message, String permission) {
        if (server == null || message == null) return 0;
        int count = 0;
        for (Player player : server.getOnlinePlayers()) if (player != null && (permission == null || permission.isEmpty() || player.hasPermission(permission))) { player.sendMessage(message); count++; }
        return count;
    }

    public static int broadcastMessage(String message) {
        return server.broadcastMessage(message);
    }

    public static boolean dispatchCommand(CommandSender sender, String command) {
        return server.dispatchCommand(sender, command);
    }

    /** Reloads datapack-backed server data through Steel's existing reload job. */
    public static void reloadData() {
        server.dispatchCommand(server.getConsoleSender(), "reload");
    }

    /** Resolves the selector forms used by server-redirect's target command. */
    public static List<org.bukkit.entity.Entity> selectEntities(
            CommandSender sender, String selector) {
        if (selector == null || selector.length() < 2 || selector.charAt(0) != '@') {
            return List.of();
        }
        char kind = selector.charAt(1);
        List<org.bukkit.entity.Entity> all = new java.util.ArrayList<>();
        for (World world : getWorlds()) {
            all.addAll(world.getEntities());
        }
        if (kind == 'a') {
            all.removeIf(entity -> !(entity instanceof Player));
        } else if (kind == 's') {
            if (sender instanceof org.bukkit.entity.Entity entity) return List.of(entity);
            return List.of();
        } else if (kind != 'e') {
            return List.of();
        }
        return java.util.Collections.unmodifiableList(all);
    }

    public static OfflinePlayer getOfflinePlayer(String name) {
        return server.getOfflinePlayer(name);
    }

    public static OfflinePlayer getOfflinePlayer(UUID id) {
        return server.getOfflinePlayer(id);
    }

    public static OfflinePlayer[] getOfflinePlayers() { return server.getOfflinePlayers(); }

    public static org.bukkit.boss.BossBar createBossBar(
            String title,
            org.bukkit.boss.BarColor color,
            org.bukkit.boss.BarStyle style,
            org.bukkit.boss.BarFlag... flags) {
        return server.createBossBar(title, color, style, flags);
    }

    public static io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler
            getGlobalRegionScheduler() {
        return server.getGlobalRegionScheduler();
    }

    public static io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() {
        return server.getRegionScheduler();
    }

    public static io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() {
        return server.getAsyncScheduler();
    }
}
