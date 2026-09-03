package foton;

import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import java.util.logging.Logger;
import org.bukkit.Server;
import org.bukkit.World;
import org.bukkit.command.CommandSender;
import org.bukkit.command.ConsoleCommandSender;
import org.bukkit.entity.Player;
import org.bukkit.event.Listener;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginManager;
import org.bukkit.plugin.ServicesManager;
import org.bukkit.plugin.SimpleServicesManager;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.scheduler.BukkitScheduler;

/** The one Server, answering out of Foton. */
public final class FotonServer implements Server {
    private static final SimpleBanList<String> NAME_BANS = new SimpleBanList<>();
    private static final SimpleBanList<String> IP_BANS = new SimpleBanList<>();
    private static final org.bukkit.command.CommandMap COMMAND_MAP = (fallback, command) -> {
        if (command == null) return false;
        CommandMap.register(command);
        return true;
    };
    private final PluginManager plugins = new Plugins();
    private final Messenger channels = new FotonMessenger();
    private final BukkitScheduler scheduler = new FotonScheduler();
    private final ServicesManager services = new SimpleServicesManager();
    private final org.bukkit.scoreboard.ScoreboardManager scoreboardManager = new FotonScoreboardManager();
    private final Logger logger = Logger.getLogger("Foton");

    @Override
    public PluginManager getPluginManager() {
        return plugins;
    }

    @Override public org.bukkit.command.PluginCommand getPluginCommand(String name) {
        org.bukkit.command.Command command = CommandMap.get(name);
        return command instanceof org.bukkit.command.PluginCommand
            ? (org.bukkit.command.PluginCommand) command : null;
    }

    @Override
    public Messenger getMessenger() {
        return channels;
    }

    @Override
    public BukkitScheduler getScheduler() {
        return scheduler;
    }

    @Override
    public ServicesManager getServicesManager() {
        return services;
    }

    @Override
    public boolean isPrimaryThread() {
        return Native.isPrimaryThread();
    }

    @Override
    public void savePlayers() { Native.savePlayers(); }

    @Override
    public void shutdown() {
        Native.shutdown();
    }

    @Override
    public Collection<? extends Player> getOnlinePlayers() {
        List<Player> online = new ArrayList<>();
        String[] ids = Native.onlinePlayerIds();
        if (ids == null) return online;
        for (String id : ids) {
            UUID parsed = Native.parse(id);
            if (parsed != null) {
                online.add(new FotonPlayer(parsed));
            }
        }
        return java.util.Collections.unmodifiableList(online);
    }

    @Override
    public java.util.Set<org.bukkit.OfflinePlayer> getWhitelistedPlayers() {
        java.util.Set<org.bukkit.OfflinePlayer> result = new java.util.LinkedHashSet<>();
        String[] ids = Native.knownPlayerIds();
        if (ids == null) return result;
        for (String id : ids) {
            UUID parsed = Native.parse(id);
            if (parsed != null && Native.offlineIsWhitelisted(id)) {
                result.add(new FotonOfflinePlayer(parsed, null));
            }
        }
        return java.util.Collections.unmodifiableSet(result);
    }

    /** Bukkit matches a partial name here, and plugins depend on it: `/msg ad`
     * finding Ada is this method, not the command that calls it. */
    @Override
    public Player getPlayer(String name) {
        if (name == null || name.isEmpty()) {
            return null;
        }
        Player exact = getPlayerExact(name);
        if (exact != null) {
            return exact;
        }
        String wanted = name.toLowerCase(Locale.ROOT);
        Player best = null;
        int shortest = Integer.MAX_VALUE;
        for (Player player : getOnlinePlayers()) {
            String candidate = player.getName().toLowerCase(Locale.ROOT);
            if (candidate.startsWith(wanted) && candidate.length() < shortest) {
                best = player;
                shortest = candidate.length();
            }
        }
        return best;
    }

    @Override
    public Player getPlayerExact(String name) {
        String id = Native.playerIdByName(name);
        if (id == null) {
            return null;
        }
        UUID parsed = Native.parse(id);
        return parsed == null ? null : new FotonPlayer(parsed);
    }

    @Override
    public Player getPlayer(UUID id) {
        if (id == null) {
            return null;
        }
        FotonPlayer player = new FotonPlayer(id);
        return player.isOnline() ? player : null;
    }

    @Override
    public String getName() {
        return Native.serverName();
    }
    @Override public String getMotd() { return Native.serverMotd(); }

    @Override
    public String getVersion() {
        return Native.serverBrand();
    }

    /** The API version a plugin checks against, not Foton's own.
     *
     * A plugin reading this is asking "which Bukkit am I talking to", and
     * answering with Foton's version number would tell it something true about
     * the wrong question.
     */
    @Override
    public String getBukkitVersion() {
        return Native.serverVersion();
    }

    @Override
    public String getMinecraftVersion() {
        return Native.minecraftVersion();
    }

    @Override
    public org.bukkit.inventory.Recipe getRecipe(org.bukkit.NamespacedKey key) {
        if (key == null) return null;
        String encoded = Native.recipeResult(key.toString());
        if (encoded == null || encoded.isEmpty()) return null;
        String[] fields = encoded.split("\\|", -1);
        if (fields.length != 2) return null;
        org.bukkit.Material material = org.bukkit.Material.matchMaterial(fields[0]);
        try {
            return material == null ? null : new FotonRecipe(key,
                new org.bukkit.inventory.ItemStack(material, Integer.parseInt(fields[1])));
        } catch (NumberFormatException ignored) { return null; }
    }

    private java.util.List<org.bukkit.inventory.Recipe> craftingRecipes() {
        java.util.ArrayList<org.bukkit.inventory.Recipe> result = new java.util.ArrayList<>();
        String[] entries = Native.recipeList();
        if (entries == null) return result;
        for (String entry : entries) {
            String[] fields = entry.split("\\|", -1);
            if (fields.length != 3) continue;
            org.bukkit.NamespacedKey key = org.bukkit.NamespacedKey.fromString(fields[0]);
            org.bukkit.Material material = org.bukkit.Material.matchMaterial(fields[1]);
            if (key == null || material == null) continue;
            try {
                result.add(new FotonRecipe(key,
                    new org.bukkit.inventory.ItemStack(material, Integer.parseInt(fields[2]))));
            } catch (NumberFormatException ignored) { }
        }
        return result;
    }

    @Override public java.util.List<org.bukkit.inventory.Recipe> getRecipesFor(org.bukkit.inventory.ItemStack result) {
        if (result == null) return java.util.Collections.emptyList();
        java.util.ArrayList<org.bukkit.inventory.Recipe> matches = new java.util.ArrayList<>();
        for (org.bukkit.inventory.Recipe recipe : craftingRecipes()) {
            if (recipe.getResult().isSimilar(result)) matches.add(recipe);
        }
        return matches;
    }

    @Override public java.util.Iterator<org.bukkit.inventory.Recipe> recipeIterator() {
        return craftingRecipes().iterator();
    }

    @Override public boolean removeRecipe(org.bukkit.NamespacedKey key) {
        return key != null && Native.recipeRemove(key.toString());
    }

    @Override public boolean addRecipe(org.bukkit.inventory.Recipe recipe) {
        if (recipe instanceof org.bukkit.inventory.ShapedRecipe shaped) {
            if (shaped.getKey() == null || recipe.getResult() == null) return false;
            java.util.ArrayList<String> ingredients = new java.util.ArrayList<>();
            for (java.util.Map.Entry<Character, org.bukkit.inventory.RecipeChoice> entry : shaped.getChoiceMap().entrySet()) {
                org.bukkit.inventory.RecipeChoice choice = entry.getValue();
                if (!(choice instanceof org.bukkit.inventory.RecipeChoice.MaterialChoice materialChoice)
                        || materialChoice.getChoices().size() != 1) return false;
                ingredients.add(entry.getKey() + "=minecraft:" + materialChoice.getChoices().get(0).getKeyName());
            }
            org.bukkit.inventory.ItemStack result = recipe.getResult();
            return Native.recipeAddShaped(shaped.getKey().toString(),
                "minecraft:" + result.getType().getKeyName(), result.getAmount(), shaped.getShape(),
                ingredients.toArray(new String[0]));
        }
        if (!(recipe instanceof org.bukkit.inventory.ShapelessRecipe shapeless)
                || shapeless.getKey() == null || recipe.getResult() == null) return false;
        java.util.List<String> ingredients = new java.util.ArrayList<>();
        for (org.bukkit.inventory.RecipeChoice choice : shapeless.getChoiceList()) {
            if (!(choice instanceof org.bukkit.inventory.RecipeChoice.MaterialChoice materialChoice)
                    || materialChoice.getChoices().size() != 1) return false;
            org.bukkit.Material material = materialChoice.getChoices().get(0);
            ingredients.add("minecraft:" + material.getKeyName());
        }
        org.bukkit.inventory.ItemStack result = recipe.getResult();
        return Native.recipeAddShapeless(shapeless.getKey().toString(),
            "minecraft:" + result.getType().getKeyName(), result.getAmount(),
            ingredients.toArray(new String[0]));
    }

    @Override
    public boolean getOnlineMode() {
        return Native.onlineMode();
    }
    /** Steel accepts connections directly; no proxy forwarding is configured. */
    @Override
    public io.papermc.paper.configuration.ServerConfiguration getServerConfig() {
        return new io.papermc.paper.configuration.ServerConfiguration(false);
    }

    @Override
    public int getMaxPlayers() {
        return Native.maxPlayers();
    }
    @Override public String getIp() { return org.bukkit.Bukkit.getIp(); }
    @Override public int getViewDistance() { return Native.serverViewDistance(); }
    @Override public int getSimulationDistance() { return Native.serverSimulationDistance(); }
    @Override public boolean getAllowFlight() { return Native.serverAllowFlight(); }
    @Override public org.bukkit.GameMode getDefaultGameMode() { try { return org.bukkit.GameMode.valueOf(Native.serverDefaultGameMode()); } catch (Exception ignored) { return org.bukkit.GameMode.SURVIVAL; } }

    @Override
    public Logger getLogger() {
        return logger;
    }

    @Override public org.bukkit.BanList<?> getBanList(org.bukkit.BanList.Type type) {
        return type == org.bukkit.BanList.Type.IP ? IP_BANS : NAME_BANS;
    }

    static boolean isNameBanned(String name) { return NAME_BANS.isBannedIgnoreCase(name); }
    static boolean isIpBanned(String address) { return IP_BANS.isBanned(address); }

    @Override
    public ConsoleCommandSender getConsoleSender() {
        return ConsoleSender.INSTANCE;
    }

    @Override
    public List<World> getWorlds() {
        List<World> worlds = new ArrayList<>();
        String[] names = Native.worldNames();
        if (names == null) return worlds;
        for (String name : names) {
            worlds.add(new FotonWorld(name));
        }
        return java.util.Collections.unmodifiableList(worlds);
    }

    @Override
    public org.bukkit.scoreboard.ScoreboardManager getScoreboardManager() {
        return scoreboardManager;
    }

    @Override
    public World getWorld(String name) {
        if (name == null) {
            return null;
        }
        String[] names = Native.worldNames();
        if (names == null) return null;
        for (String candidate : names) {
            if (candidate.equals(name)) {
                return new FotonWorld(candidate);
            }
        }
        return null;
    }

    @Override
    public int broadcastMessage(String message) {
        return Native.broadcast(message);
    }

    @Override
    public void sendPluginMessage(Plugin source, String channel, byte[] message) {
        for (Player player : getOnlinePlayers()) {
            player.sendPluginMessage(source, channel, message);
        }
    }

    @Override
    public org.bukkit.boss.BossBar createBossBar(
            String title,
            org.bukkit.boss.BarColor color,
            org.bukkit.boss.BarStyle style,
            org.bukkit.boss.BarFlag... flags) {
        return new FotonBossBar(title, color, style, flags);
    }

    /** Runs a command as somebody. Only plugin commands: Foton's own
     * dispatcher runs on the tick thread and this can be called from anywhere,
     * so reaching into it from here would be the race the scheduler exists to
     * prevent. */
    @Override
    public boolean dispatchCommand(CommandSender sender, String command) {
        return CommandMap.dispatch(sender, command);
    }

    @Override public org.bukkit.command.CommandMap getCommandMap() { return COMMAND_MAP; }

    @Override
    public org.bukkit.OfflinePlayer getOfflinePlayer(String name) {
        if (name == null) throw new IllegalArgumentException("name");
        Player online = getPlayerExact(name);
        if (online != null) return new FotonOfflinePlayer(online.getUniqueId(), name);
        String id = Native.knownPlayerIdByName(name);
        if (id != null) {
            try { return new FotonOfflinePlayer(UUID.fromString(id), name); }
            catch (IllegalArgumentException ignored) { /* fall through to deterministic identity */ }
        }
        UUID offlineId = UUID.nameUUIDFromBytes(
            ("OfflinePlayer:" + name).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        return new FotonOfflinePlayer(offlineId, name);
    }

    @Override
    public org.bukkit.OfflinePlayer getOfflinePlayer(UUID id) {
        if (id == null) throw new IllegalArgumentException("id");
        return new FotonOfflinePlayer(id, null);
    }

    @Override public org.bukkit.OfflinePlayer[] getOfflinePlayers() {
        String[] ids = Native.knownPlayerIds();
        List<org.bukkit.OfflinePlayer> players = new ArrayList<>(ids.length);
        for (String value : ids) {
            try {
                players.add(new FotonOfflinePlayer(UUID.fromString(value), null));
            } catch (IllegalArgumentException ignored) {
                // Do not leak an invalid UUID into a Bukkit handle.
            }
        }
        return players.toArray(new org.bukkit.OfflinePlayer[0]);
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler
            getGlobalRegionScheduler() {
        return FotonRegionSchedulers.GLOBAL;
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.RegionScheduler getRegionScheduler() {
        return FotonRegionSchedulers.REGION;
    }

    @Override
    public io.papermc.paper.threadedregions.scheduler.AsyncScheduler getAsyncScheduler() {
        return FotonRegionSchedulers.ASYNC;
    }

    private static final class Plugins implements PluginManager {
        private final java.util.Map<String, org.bukkit.permissions.Permission> permissions = new java.util.concurrent.ConcurrentHashMap<>();
        @Override public void registerEvents(Listener listener, Plugin plugin) {
            EventBridge.register(listener, plugin);
        }

        @Override public Plugin getPlugin(String name) {
            return PluginHost.byName(name);
        }

        @Override public Plugin[] getPlugins() {
            return PluginHost.all();
        }
        @Override public org.bukkit.permissions.Permission getPermission(String name) {
            return name == null ? null : permissions.get(name.toLowerCase(java.util.Locale.ROOT));
        }
        @Override public void addPermission(org.bukkit.permissions.Permission permission) {
            if (permission != null) permissions.putIfAbsent(permission.getName().toLowerCase(java.util.Locale.ROOT), permission);
        }
        @Override public void removePermission(String name) {
            if (name != null) permissions.remove(name.toLowerCase(java.util.Locale.ROOT));
        }

        @Override public void callEvent(org.bukkit.event.Event event) {
            EventBridge.dispatch(event);
        }

        @Override public boolean isPluginEnabled(String name) {
            Plugin plugin = PluginHost.byName(name);
            return plugin != null && plugin.isEnabled();
        }

        @Override public boolean isPluginEnabled(Plugin plugin) {
            return plugin != null && plugin.isEnabled();
        }

        @Override public void disablePlugin(Plugin plugin) {
            PluginHost.disable(plugin);
        }

        @Override public void registerEvent(
                Class<? extends org.bukkit.event.Event> event,
                Listener listener,
                org.bukkit.event.EventPriority priority,
                org.bukkit.plugin.EventExecutor executor,
                Plugin plugin) {
            EventBridge.register(listener, event, priority, executor, plugin);
        }

        @Override public void registerEvent(
                Class<? extends org.bukkit.event.Event> event,
                Listener listener,
                org.bukkit.event.EventPriority priority,
                org.bukkit.plugin.EventExecutor executor,
                Plugin plugin,
                boolean ignoreCancelled) {
            EventBridge.register(
                listener, event, priority, executor, plugin, ignoreCancelled);
        }
    }

}
