package foton;

import org.bukkit.Bukkit;

import org.bukkit.command.CommandSender;
import org.bukkit.Location;
import org.bukkit.inventory.ItemStack;
import org.bukkit.event.inventory.PrepareGrindstoneEvent;
import org.bukkit.event.inventory.PrepareItemCraftEvent;
import org.bukkit.event.block.CrafterCraftEvent;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.bukkit.entity.Player;
import org.bukkit.entity.Hanging;
import org.bukkit.event.Cancellable;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.block.BlockFromToEvent;
import org.bukkit.event.entity.EntityPickupItemEvent;
import io.papermc.paper.event.entity.EntityPushedByEntityAttackEvent;
import org.bukkit.event.entity.ItemSpawnEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.event.player.PlayerMoveEvent;
import org.bukkit.event.player.PlayerAttemptPickupItemEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.EventExecutor;

/** Where a plugin's annotated handlers meet Foton's events.
 *
 * A plugin does not register handlers, it annotates methods and hands over the
 * object; finding them is reflection's job and it stays here. Foton calls the
 * `fire*` methods below when something happens, and reads the result to learn
 * what the plugins decided.
 *
 * The result travels back as a return value rather than through a callback.
 * That keeps the whole exchange one call deep: Foton asks, Java answers, and
 * nothing on either side has to hold a reference to the other's objects for
 * longer than the call.
 */
public final class EventBridge {
    private static final java.util.concurrent.ConcurrentHashMap<java.util.UUID, DamageRecord> LAST_DAMAGE = new java.util.concurrent.ConcurrentHashMap<>();
    private record DamageRecord(org.bukkit.event.entity.EntityDamageEvent event, long tick) {}

    private static final Map<Class<?>, List<Handler>> handlers = new HashMap<>();

    private EventBridge() {}

    /** Reflects over a listener and remembers every annotated handler. */
    public static void register(Listener listener, Plugin plugin) {
        for (Method method : listener.getClass().getMethods()) {
            EventHandler annotation = method.getAnnotation(EventHandler.class);
            if (annotation == null || method.getParameterCount() != 1) {
                continue;
            }
            Class<?> event = method.getParameterTypes()[0];
            method.setAccessible(true);
            handlers.computeIfAbsent(event, key -> new ArrayList<>())
                .add(new Handler(listener, method, annotation.priority(),
                    annotation.ignoreCancelled(), plugin));
            handlers.get(event).sort(Comparator.comparing(handler -> handler.priority));
        }
    }

    /** Forgets everything one plugin registered. */
    public static void unregister(Plugin plugin) {
        for (List<Handler> list : handlers.values()) {
            list.removeIf(handler -> handler.plugin == plugin);
        }
    }

    /** Forgets everything one listener object registered. */
    public static void unregister(Listener listener) {
        for (List<Handler> list : handlers.values()) {
            list.removeIf(handler -> handler.listener == listener);
        }
    }

    /** Forgets every handler on the server. */
    public static void unregisterAll() {
        handlers.clear();
    }

    /** Registers one handler by hand, for a plugin that builds its listeners
     * at runtime rather than annotating them. */
    public static void register(
            Listener listener, Class<?> event, EventPriority priority, EventExecutor executor,
            Plugin plugin) {
        register(listener, event, priority, executor, plugin, false);
    }

    /** Registers one hand-built handler with its cancellation policy. */
    public static void register(
            Listener listener, Class<?> event, EventPriority priority, EventExecutor executor,
            Plugin plugin, boolean ignoreCancelled) {
        handlers.computeIfAbsent(event, key -> new ArrayList<>())
            .add(new Handler(listener, null, executor, priority, ignoreCancelled, plugin));
        handlers.get(event).sort(Comparator.comparing(handler -> handler.priority));
    }

    /** Runs every handler registered for an event's type, in priority order.
     *
     * Public because a plugin can fire its own events through
     * `PluginManager#callEvent`, and eighteen of the fifty-nine plugins
     * surveyed do -- an event a plugin defines reaches other plugins' handlers
     * by exactly this path.
     */
    public static void dispatch(Object event) {
        List<Handler> list = new ArrayList<>();
        for (Map.Entry<Class<?>, List<Handler>> entry : handlers.entrySet()) {
            if (entry.getKey().isAssignableFrom(event.getClass())) {
                list.addAll(entry.getValue());
            }
        }
        if (list.isEmpty()) {
            return;
        }
        list.sort(Comparator.comparing(handler -> handler.priority));
        boolean cancellable = event instanceof Cancellable;
        for (Handler handler : List.copyOf(list)) {
            if (cancellable && ((Cancellable) event).isCancelled() && handler.ignoreCancelled) {
                continue;
            }
            try {
                handler.call(event);
            } catch (Throwable error) {
                // One plugin throwing must not stop the others, and must not
                // reach Foton: an exception crossing JNI is a crash, not an
                // error message.
                System.out.println("[events] " + handler.plugin.getName() + " threw in "
                    + handler.name() + ": " + rootOf(error));
            }
        }
    }

    private static Throwable rootOf(Throwable error) {
        return error.getCause() == null ? error : error.getCause();
    }

    private static Player player(String uuid) {
        UUID parsed = Native.parse(uuid);
        return parsed == null ? null : new FotonPlayer(parsed);
    }

    /** A player joined. Returns what to announce, or null to announce nothing. */
    public static String fireJoin(String uuid, String message) {
        PlayerJoinEvent event = new PlayerJoinEvent(player(uuid), message);
        dispatch(event);
        return event.getJoinMessage();
    }

    public static String fireLogin(String uuid) {
        PlayerLoginEvent event = new PlayerLoginEvent(player(uuid));
        dispatch(event);
        return event.isCancelled() ? event.getKickMessage() : "";
    }

    public static String fireAsyncPreLogin(String name, String uuid, String address) {
        try {
            org.bukkit.event.player.AsyncPlayerPreLoginEvent event = new org.bukkit.event.player.AsyncPlayerPreLoginEvent(
                name, UUID.fromString(uuid), java.net.InetAddress.getByName(address));
            if (FotonServer.isNameBanned(name) || FotonServer.isIpBanned(address)) {
                event.disallow(org.bukkit.event.player.AsyncPlayerPreLoginEvent.Result.KICK_BANNED,
                    "You are banned from this server.");
            } else {
                dispatch(event);
            }
            if (event.getLoginResult() == org.bukkit.event.player.AsyncPlayerPreLoginEvent.Result.ALLOWED) return "";
            return event.getLoginResult().name() + "\u001f" + event.getKickMessage();
        } catch (Exception ignored) { return ""; }
    }

    /** A player is attempting an item interaction. */
    public static boolean fireInteract(String uuid) {
        org.bukkit.event.player.PlayerInteractEvent event =
            new org.bukkit.event.player.PlayerInteractEvent(player(uuid),
                org.bukkit.event.block.Action.RIGHT_CLICK_AIR,
                null, null, null);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireInteractEntity(String playerUuid, String entityUuid) {
        org.bukkit.event.player.PlayerInteractEntityEvent event =
            new org.bukkit.event.player.PlayerInteractEntityEvent(
                player(playerUuid), foton.FotonEntity.handle(Native.parse(entityUuid)));
        dispatch(event);
        return !event.isCancelled();
    }

    /** Gives plugins a snapshot-backed crafting preview and returns their changes. */
    public static String firePrepareCraft(String uuid, String matrix, String result, boolean repair) {
        FotonPlayer player=(FotonPlayer) player(uuid); String[] encoded=matrix == null ? new String[0] : matrix.split("\\u001e", -1);
        ItemStack[] slots=new ItemStack[encoded.length]; for(int i=0;i<encoded.length;i++) slots[i]=FotonInventory.decode(encoded[i]);
        FotonCraftingInventory inventory=new FotonCraftingInventory(uuid); inventory.setMatrix(slots);
        PrepareItemCraftEvent event=new PrepareItemCraftEvent(new FotonInventoryView(player, inventory), inventory, FotonInventory.decode(result), repair); dispatch(event);
        StringBuilder out=new StringBuilder(); for(int i=0;i<slots.length;i++){if(i>0)out.append("\\u001e"); out.append(FotonInventory.encode(inventory.getItem(i)));} out.append("\\u001f").append(FotonInventory.encode(inventory.getResult())); return out.toString();
    }

    /** Gives plugins a snapshot-backed grindstone preview and returns their changes. */
    public static String firePrepareGrindstone(String uuid, String upper, String lower, String result) {
        FotonPlayer player = (FotonPlayer) player(uuid);
        ItemStack[] slots = new ItemStack[] {
            FotonInventory.decode(upper), FotonInventory.decode(lower), FotonInventory.decode(result)
        };
        FotonGrindstoneInventory inventory = new FotonGrindstoneInventory(uuid, slots);
        PrepareGrindstoneEvent event = new PrepareGrindstoneEvent(
            new FotonInventoryView(player, inventory), inventory, slots[2]);
        dispatch(event);
        return FotonInventory.encode(inventory.getUpperItem()) + "\u001f"
            + FotonInventory.encode(inventory.getLowerItem()) + "\u001f"
            + FotonInventory.encode(event.getResult());
    }

    public static String fireCrafterCraft(String world, int x, int y, int z, String recipeKey, String result, String remaining) {
        org.bukkit.World bukkitWorld = new FotonWorld(world);
        org.bukkit.inventory.ItemStack crafted = FotonInventory.decode(result);
        org.bukkit.inventory.CraftingRecipe recipe = new FotonCraftingRecipe(
            org.bukkit.NamespacedKey.fromString(recipeKey), crafted);
        java.util.List<org.bukkit.inventory.ItemStack> items = new java.util.ArrayList<>();
        if (remaining != null && !remaining.isEmpty()) {
            for (String encoded : remaining.split("\u001e", -1)) items.add(FotonInventory.decode(encoded));
        }
        CrafterCraftEvent event = new CrafterCraftEvent(
            new FotonBlock(bukkitWorld, x, y, z), recipe, crafted, items);
        dispatch(event);
        StringBuilder out = new StringBuilder();
        out.append(event.isCancelled() ? "1" : "0").append("\u001f")
           .append(FotonInventory.encode(event.getResult())).append("\u001f");
        for (int i = 0; i < event.getRemainingItems().size(); i++) {
            if (i > 0) out.append("\u001e");
            out.append(FotonInventory.encode(event.getRemainingItems().get(i)));
        }
        return out.toString();
    }

    public static boolean fireInventoryClick(String uuid, String item, String click) {
        return fireInventoryClick(uuid, item, "", click, -1);
    }

    public static boolean fireInventoryOpen(String uuid) {
        org.bukkit.entity.Player player = player(uuid);
        org.bukkit.event.inventory.InventoryOpenEvent event =
            new org.bukkit.event.inventory.InventoryOpenEvent(player);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityTarget(String entity, String target) {
        org.bukkit.entity.Entity source = FotonEntity.handle(Native.parse(entity));
        org.bukkit.entity.Entity selected = target == null || target.isEmpty()
            ? null : FotonEntity.handle(Native.parse(target));
        org.bukkit.event.entity.EntityTargetEvent event =
            new org.bukkit.event.entity.EntityTargetEvent(source, selected);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean firePotionEffect(String entity, String type, int oldDuration, int oldAmplifier, int newDuration, int newAmplifier, String action) {
        org.bukkit.entity.LivingEntity living = (org.bukkit.entity.LivingEntity) FotonEntity.handle(Native.parse(entity));
        org.bukkit.potion.PotionEffect oldEffect = oldDuration < 0 ? null : new org.bukkit.potion.PotionEffect(org.bukkit.potion.PotionEffectType.getByName(type), oldDuration, oldAmplifier);
        org.bukkit.potion.PotionEffect newEffect = newDuration < 0 ? null : new org.bukkit.potion.PotionEffect(org.bukkit.potion.PotionEffectType.getByName(type), newDuration, newAmplifier);
        org.bukkit.event.entity.EntityPotionEffectEvent event = new org.bukkit.event.entity.EntityPotionEffectEvent(living, oldEffect, newEffect, org.bukkit.event.entity.EntityPotionEffectEvent.Action.valueOf(action));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityMount(String entity, String vehicle) { org.bukkit.event.entity.EntityMountEvent event = new org.bukkit.event.entity.EntityMountEvent(FotonEntity.handle(Native.parse(entity)), FotonEntity.handle(Native.parse(vehicle))); dispatch(event); return !event.isCancelled(); }

    public static String fireExpBottle(String entity, int experience) {
        org.bukkit.event.entity.ExpBottleEvent event = new org.bukkit.event.entity.ExpBottleEvent(new FotonThrownExpBottle(Native.parse(entity)), experience);
        dispatch(event);
        return (event.isCancelled() ? "0" : "1") + "|" + event.getExperience();
    }

    public static boolean fireProjectileLaunch(String shooterUuid, String projectileUuid) {
        org.bukkit.entity.Entity shooter = FotonEntity.handle(Native.parse(shooterUuid));
        org.bukkit.entity.Projectile projectile = new FotonProjectile(Native.parse(projectileUuid));
        org.bukkit.event.entity.ProjectileLaunchEvent event =
            new org.bukkit.event.entity.ProjectileLaunchEvent(projectile, shooter);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireInventoryClick(String uuid, String item, String cursor, String click, int rawSlot) {
        org.bukkit.event.inventory.InventoryClickEvent event =
            new org.bukkit.event.inventory.InventoryClickEvent(player(uuid), FotonInventory.decode(item), FotonInventory.decode(cursor), clickType(click), rawSlot);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireInventoryDrag(String uuid, String slots, String oldCursor, String type) {
        java.util.Set<Integer> raw = new java.util.LinkedHashSet<>();
        if (slots != null && !slots.isEmpty()) for (String value : slots.split(",")) try { raw.add(Integer.parseInt(value)); } catch (RuntimeException ignored) { }
        org.bukkit.event.inventory.InventoryDragEvent.DragType dragType = "Right".equalsIgnoreCase(type)
            ? org.bukkit.event.inventory.InventoryDragEvent.DragType.SINGLE : org.bukkit.event.inventory.InventoryDragEvent.DragType.EVEN;
        org.bukkit.event.inventory.InventoryDragEvent event = new org.bukkit.event.inventory.InventoryDragEvent(player(uuid), raw, FotonInventory.decode(oldCursor), dragType);
        dispatch(event); return !event.isCancelled();
    }

    private static org.bukkit.event.entity.EntityDamageEvent.DamageCause damageCause(String name) {
        try { return org.bukkit.event.entity.EntityDamageEvent.DamageCause.valueOf(name); }
        catch (IllegalArgumentException | NullPointerException error) { return org.bukkit.event.entity.EntityDamageEvent.DamageCause.CUSTOM; }
    }

    private static org.bukkit.event.inventory.ClickType clickType(String name) {
        try { return org.bukkit.event.inventory.ClickType.valueOf(name); }
        catch (IllegalArgumentException | NullPointerException error) { return org.bukkit.event.inventory.ClickType.UNKNOWN; }
    }

    public static void fireEntityRemove(String entity) {
        org.bukkit.entity.Entity handle = FotonEntity.handle(Native.parse(entity));
        dispatch(new org.bukkit.event.entity.EntityRemoveFromWorldEvent(handle));
        dispatch(new com.destroystokyo.paper.event.entity.EntityRemoveFromWorldEvent(handle));
    }

    public static boolean fireItemSpawn(String entity, String world,
            double x, double y, double z, String item) {
        ItemSpawnEvent event = new ItemSpawnEvent(
            new FotonItem(Native.parse(entity)),
            new org.bukkit.Location(new FotonWorld(world), x, y, z));
        // The item stack is encoded for the native side; the entity handle
        // resolves it through the pending-spawn registry during dispatch.
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityPickup(String entity, String item) {
        org.bukkit.entity.LivingEntity living = new FotonLivingEntity(Native.parse(entity));
        org.bukkit.entity.Item itemHandle = new FotonItem(Native.parse(item));
        org.bukkit.entity.Player player = null;
        try { player = Bukkit.getServer().getPlayer(UUID.fromString(entity)); } catch (IllegalArgumentException ignored) { }
        if (player != null) {
            PlayerAttemptPickupItemEvent attempt = new PlayerAttemptPickupItemEvent(player, itemHandle);
            dispatch(attempt);
            if (attempt.isCancelled()) return false;
        }
        EntityPickupItemEvent event = new EntityPickupItemEvent(living, itemHandle);
        dispatch(event);
        return !event.isCancelled();
    }

    private static org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason spawnReason(String reason) {
        if (reason == null) return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.DEFAULT;
        try {
            return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.valueOf(
                reason.toUpperCase(java.util.Locale.ROOT));
        } catch (IllegalArgumentException error) {
            return org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason.CUSTOM;
        }
    }

    public static boolean firePreCreatureSpawn(String world, double x, double y, double z, String type, String reason) {
        org.bukkit.entity.EntityType entityType = org.bukkit.entity.EntityType.fromName(type);
        if (entityType == null) return true;
        org.bukkit.event.entity.CreatureSpawnEvent.SpawnReason spawnReason = spawnReason(reason);
        com.destroystokyo.paper.event.entity.PreCreatureSpawnEvent event =
            new com.destroystokyo.paper.event.entity.PreCreatureSpawnEvent(
                new org.bukkit.Location(new FotonWorld(world), x, y, z), entityType, spawnReason);
        dispatch(event);
        return !event.isCancelled() && !event.shouldAbortSpawn();
    }
    public static String fireEntityPortal(String entity, String fromWorld, double fx, double fy, double fz, String toWorld, double tx, double ty, double tz, String portalType) {
        try {
            org.bukkit.entity.Entity value = FotonEntity.handle(java.util.UUID.fromString(entity));
            org.bukkit.Location from = new org.bukkit.Location(new FotonWorld(fromWorld), fx, fy, fz);
            org.bukkit.Location to = new org.bukkit.Location(new FotonWorld(toWorld), tx, ty, tz);
            org.bukkit.PortalType type; try { type = org.bukkit.PortalType.valueOf(portalType); } catch (IllegalArgumentException ex) { type = org.bukkit.PortalType.CUSTOM; }
            org.bukkit.event.entity.EntityPortalEvent event = new org.bukkit.event.entity.EntityPortalEvent(value, from, to, 128, true, 16, type);
            dispatch(event);
            if (event.isCancelled()) return "!";
            org.bukkit.Location destination = event.getTo();
            if (destination == null || destination.getWorld() == null) return "!";
            return destination.getWorld().getName()+"|"+destination.getX()+"|"+destination.getY()+"|"+destination.getZ();
        } catch (RuntimeException ex) { return "!"; }
    }
    public static boolean fireCreatureSpawn(String entity, String world, double x, double y, double z, String reason) {
        org.bukkit.entity.LivingEntity living = new FotonLivingEntity(Native.parse(entity));
        org.bukkit.event.entity.CreatureSpawnEvent event =
            new org.bukkit.event.entity.CreatureSpawnEvent(
                living, new org.bukkit.Location(new FotonWorld(world), x, y, z),
                spawnReason(reason));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityRegainHealth(String entity, float amount) {
        org.bukkit.event.entity.EntityRegainHealthEvent event =
            new org.bukkit.event.entity.EntityRegainHealthEvent(new FotonLivingEntity(Native.parse(entity)), amount);
        dispatch(event);
        return !event.isCancelled();
    }

    public static void setLastDamageCause(java.util.UUID entity, org.bukkit.event.entity.EntityDamageEvent event) {
        if (entity == null) return;
        if (event == null) LAST_DAMAGE.remove(entity);
        else LAST_DAMAGE.put(entity, new DamageRecord(event, Bukkit.getCurrentTick()));
    }

    public static org.bukkit.event.entity.EntityDamageEvent lastDamageCause(java.util.UUID entity) {
        DamageRecord record = LAST_DAMAGE.get(entity);
        if (record == null || Bukkit.getCurrentTick() - record.tick() > 40) {
            LAST_DAMAGE.remove(entity, record);
            return null;
        }
        return record.event();
    }

    public static boolean fireEntityDamage(String damager, String entity, String cause) {
        org.bukkit.entity.Entity target = FotonEntity.handle(Native.parse(entity));
        org.bukkit.event.entity.EntityDamageByEntityEvent event =
            new org.bukkit.event.entity.EntityDamageByEntityEvent(
                FotonEntity.handle(Native.parse(damager)), target,
                damageCause(cause));
        dispatch(event);
        if (target instanceof org.bukkit.entity.LivingEntity) {
            LAST_DAMAGE.put(target.getUniqueId(), new DamageRecord(event, Bukkit.getCurrentTick()));
        }
        return !event.isCancelled();
    }

    public static boolean fireEntityPushedByEntityAttack(String entity, String pushedBy) {
        EntityPushedByEntityAttackEvent event = new EntityPushedByEntityAttackEvent(
            FotonEntity.handle(Native.parse(entity)), FotonEntity.handle(Native.parse(pushedBy)));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireHangingBreak(String entity, String cause, String remover) {
        org.bukkit.entity.Entity target = FotonEntity.handle(Native.parse(entity));
        org.bukkit.entity.EntityType type = target.getType();
        if (type != org.bukkit.entity.EntityType.ITEM_FRAME
            && type != org.bukkit.entity.EntityType.GLOW_ITEM_FRAME
            && type != org.bukkit.entity.EntityType.PAINTING
            && type != org.bukkit.entity.EntityType.LEASH_KNOT) return true;
        Hanging hanging = new FotonHanging(Native.parse(entity));
        org.bukkit.event.hanging.HangingBreakEvent event = remover == null || remover.isEmpty()
            ? new org.bukkit.event.hanging.HangingBreakEvent(hanging,
                org.bukkit.event.hanging.HangingBreakEvent.RemoveCause.valueOf(
                    cause.toUpperCase(java.util.Locale.ROOT)))
            : new org.bukkit.event.hanging.HangingBreakByEntityEvent(hanging,
                FotonEntity.handle(Native.parse(remover)));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireHangingPlace(String entity, String playerUuid,
            String world, int x, int y, int z, String face) {
        org.bukkit.event.hanging.HangingPlaceEvent event =
            new org.bukkit.event.hanging.HangingPlaceEvent(
                new FotonHanging(Native.parse(entity)), player(playerUuid),
                new FotonBlock(new FotonWorld(world), x, y, z),
                org.bukkit.block.BlockFace.valueOf(face.toUpperCase(java.util.Locale.ROOT)));
        dispatch(event);
        return !event.isCancelled();
    }

    public static String fireCommandPreprocess(String uuid, String message) {
        org.bukkit.event.player.PlayerCommandPreprocessEvent event =
            new org.bukkit.event.player.PlayerCommandPreprocessEvent(player(uuid), message);
        dispatch(event);
        return event.isCancelled() ? null : event.getMessage();
    }

    /** A player left. Returns what to announce, or null to announce nothing. */
    public static String fireQuit(String uuid, String message) {
        PlayerQuitEvent event = new PlayerQuitEvent(player(uuid), message);
        dispatch(event);
        FotonMessenger.forgetPlayer(uuid);
        return event.getQuitMessage();
    }

    /** A player spoke. Returns the message, or null when a plugin stopped it. */
    public static void fireLocaleChange(String uuid, String oldLocale, String locale) {
        dispatch(new org.bukkit.event.player.PlayerLocaleChangeEvent(player(uuid), oldLocale, locale));
    }

    public static String fireChat(String uuid, String message) {
        AsyncPlayerChatEvent event = new AsyncPlayerChatEvent(player(uuid), message);
        try { event.getRecipients().addAll(org.bukkit.Bukkit.getOnlinePlayers()); }
        catch (Throwable ignored) { }
        dispatch(event);
        if (event.isCancelled()) return null;
        io.papermc.paper.event.player.AsyncChatEvent paper =
            new io.papermc.paper.event.player.AsyncChatEvent(
                player(uuid), net.kyori.adventure.text.Component.text(
                    event.getMessage() == null ? "" : event.getMessage()));
        for (Player recipient : event.getRecipients()) {
            if (recipient instanceof net.kyori.adventure.audience.Audience audience) {
                paper.viewers().add(audience);
            }
        }
        dispatch(paper);
        if (paper.isCancelled()) return null;
        event.getRecipients().clear();
        for (net.kyori.adventure.audience.Audience viewer : paper.viewers()) {
            if (viewer instanceof Player recipient) event.getRecipients().add(recipient);
        }
        String rendered = net.kyori.adventure.text.serializer.plain.PlainTextComponentSerializer
            .plainText().serialize(paper.message());
        if (event.getRecipients().isEmpty()) return rendered;
        StringBuilder answer = new StringBuilder(rendered).append('\u001e');
        for (Player recipient : event.getRecipients()) answer.append(recipient.getUniqueId()).append(',');
        return answer.toString();
    }

    /** A player is breaking a block. Returns false when a plugin stopped it. */
    public static boolean fireBlockBreak(String uuid, int x, int y, int z, String world) {
        BlockBreakEvent event =
            new BlockBreakEvent(new FotonBlock(new FotonWorld(world), x, y, z), player(uuid));
        dispatch(event);
        return !event.isCancelled();
    }

    /** A player is placing a block. Returns false when a plugin stopped it. */
    public static boolean fireBlockPlace(String uuid, int x, int y, int z, String world, String item) {
        BlockPlaceEvent event =
            new BlockPlaceEvent(new FotonBlock(new FotonWorld(world), x, y, z), player(uuid), FotonInventory.decode(item));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireBlockFromTo(String world, int x, int y, int z, int toX, int toY, int toZ) {
        BlockFromToEvent event = new BlockFromToEvent(new FotonBlock(new FotonWorld(world), x, y, z), new FotonBlock(new FotonWorld(world), toX, toY, toZ));
        dispatch(event); return !event.isCancelled();
    }

    public static String fireBlockExp(String world, int x, int y, int z, int exp) {
        org.bukkit.event.block.BlockExpEvent event = new org.bukkit.event.block.BlockExpEvent(new FotonBlock(new FotonWorld(world), x, y, z), exp);
        dispatch(event);
        return (event.isCancelled() ? "0" : "1") + "|" + event.getExpToDrop();
    }

    public static boolean fireBlockBurn(String world, int x, int y, int z) {
        org.bukkit.event.block.BlockBurnEvent event =
            new org.bukkit.event.block.BlockBurnEvent(new FotonBlock(new FotonWorld(world), x, y, z));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireBlockFade(String world, int x, int y, int z) {
        org.bukkit.event.block.BlockFadeEvent event =
            new org.bukkit.event.block.BlockFadeEvent(new FotonBlock(new FotonWorld(world), x, y, z));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireLeavesDecay(String world, int x, int y, int z) {
        org.bukkit.event.block.LeavesDecayEvent event =
            new org.bukkit.event.block.LeavesDecayEvent(new FotonBlock(new FotonWorld(world), x, y, z));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireBlockIgnite(String world, int x, int y, int z, String cause, String playerUuid) {
        org.bukkit.event.block.BlockIgniteEvent event =
            new org.bukkit.event.block.BlockIgniteEvent(
                new FotonBlock(new FotonWorld(world), x, y, z), igniteCause(cause),
                playerUuid == null ? null : new FotonPlayer(java.util.UUID.fromString(playerUuid)));
        dispatch(event);
        return !event.isCancelled();
    }
    public static boolean fireBlockFertilize(String world, int x, int y, int z, String playerUuid) {
        org.bukkit.event.block.BlockFertilizeEvent event = new org.bukkit.event.block.BlockFertilizeEvent(
            new FotonBlock(new FotonWorld(world), x, y, z),
            playerUuid == null ? null : new FotonPlayer(java.util.UUID.fromString(playerUuid)));
        dispatch(event);
        return !event.isCancelled();
    }

    private static org.bukkit.event.block.BlockIgniteEvent.IgniteCause igniteCause(String cause) {
        if (cause == null) return org.bukkit.event.block.BlockIgniteEvent.IgniteCause.OTHER;
        try {
            return org.bukkit.event.block.BlockIgniteEvent.IgniteCause.valueOf(cause);
        } catch (IllegalArgumentException ignored) {
            return org.bukkit.event.block.BlockIgniteEvent.IgniteCause.OTHER;
        }
    }

    public static void fireWorldLoad(String world) {
        FotonWorld value = new FotonWorld(world);
        dispatch(new org.bukkit.event.world.WorldInitEvent(value));
        dispatch(new org.bukkit.event.world.WorldLoadEvent(value));
    }
    public static boolean fireWorldUnload(String world) {
        org.bukkit.event.world.WorldUnloadEvent event = new org.bukkit.event.world.WorldUnloadEvent(new FotonWorld(world));
        dispatch(event);
        return !event.isCancelled();
    }
    public static boolean fireEntityResurrect(String uuid) {
        org.bukkit.event.entity.EntityResurrectEvent event = new org.bukkit.event.entity.EntityResurrectEvent(
            new FotonLivingEntity(java.util.UUID.fromString(uuid)));
        dispatch(event);
        return !event.isCancelled();
    }
    public static void fireEntityDeath(String uuid) {
        dispatch(new org.bukkit.event.entity.EntityDeathEvent(
            new FotonLivingEntity(java.util.UUID.fromString(uuid))));
    }
    public static boolean firePlayerTakeLecternBook(String uuid, String world, int x, int y, int z) {
        org.bukkit.event.player.PlayerTakeLecternBookEvent event =
            new org.bukkit.event.player.PlayerTakeLecternBookEvent(new FotonPlayer(java.util.UUID.fromString(uuid)),
                (org.bukkit.block.Lectern) new FotonBlock(new FotonWorld(world), x, y, z).getState());
        dispatch(event);
        return !event.isCancelled();
    }

    public static int fireFoodLevelChange(String player, int level) {
        org.bukkit.event.entity.FoodLevelChangeEvent event =
            new org.bukkit.event.entity.FoodLevelChangeEvent(player(player), level);
        dispatch(event);
        return event.isCancelled() ? -1 : event.getFoodLevel();
    }

    public static boolean firePlayerDropItem(String player, String item) {
        org.bukkit.event.player.PlayerDropItemEvent event =
            new org.bukkit.event.player.PlayerDropItemEvent(
                player(player), new foton.FotonItem(Native.parse(item)));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean firePlayerBucketEmpty(String player, String bucket) {
        org.bukkit.event.player.PlayerBucketEmptyEvent event =
            new org.bukkit.event.player.PlayerBucketEmptyEvent(player(player), org.bukkit.Material.matchMaterial(bucket));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean firePlayerBucketFill(String player, String world, int x, int y, int z, String bucket) {
        org.bukkit.event.player.PlayerBucketFillEvent event = new org.bukkit.event.player.PlayerBucketFillEvent(
            player(player), new FotonBlock(new FotonWorld(world), x, y, z),
            org.bukkit.Material.matchMaterial(bucket));
        dispatch(event);
        return !event.isCancelled();
    }

    public static void firePlayerItemBreak(String player, String encodedItem) {
        org.bukkit.event.player.PlayerItemBreakEvent event = new org.bukkit.event.player.PlayerItemBreakEvent(
            player(player), FotonInventory.decode(encodedItem));
        dispatch(event);
    }

    public static boolean firePlayerFish(String playerUuid, String hookUuid, String stateName) {
        org.bukkit.event.player.PlayerFishEvent.State state;
        try { state = org.bukkit.event.player.PlayerFishEvent.State.valueOf(stateName); }
        catch (RuntimeException ignored) { state = org.bukkit.event.player.PlayerFishEvent.State.FAILED_ATTEMPT; }
        org.bukkit.event.player.PlayerFishEvent event = new org.bukkit.event.player.PlayerFishEvent(
            player(playerUuid), foton.FotonEntity.handle(Native.parse(hookUuid)), state);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean firePlayerKick(String playerUuid, String reason) {
        org.bukkit.event.player.PlayerKickEvent event =
            new org.bukkit.event.player.PlayerKickEvent(player(playerUuid), reason);
        dispatch(event);
        return !event.isCancelled();
    }

    public static String firePlayerRespawn(String uuid, String encoded) { return firePlayerRespawn(uuid, encoded, false); }

    public static String firePlayerRespawn(String uuid, String encoded, boolean anchorSpawn) {
        String[] fields = encoded == null ? new String[0] : encoded.split("\\|", -1);
        org.bukkit.Location location = null;
        try {
            if (fields.length == 6) {
                location = new org.bukkit.Location(new FotonWorld(fields[0]),
                    Double.parseDouble(fields[1]), Double.parseDouble(fields[2]),
                    Double.parseDouble(fields[3]), Float.parseFloat(fields[4]),
                    Float.parseFloat(fields[5]));
            }
        } catch (RuntimeException ignored) { }
        org.bukkit.event.player.PlayerRespawnEvent event =
            new org.bukkit.event.player.PlayerRespawnEvent(player(uuid), location, false, anchorSpawn);
        dispatch(event);
        org.bukkit.Location result = event.getRespawnLocation();
        if (result == null || result.getWorld() == null) return encoded;
        return result.getWorld().getName() + "|" + result.getX() + "|" + result.getY() + "|"
            + result.getZ() + "|" + result.getYaw() + "|" + result.getPitch();
    }

    public static String firePlayerSpawnLocation(String uuid, String encoded) {
        String[] fields = encoded == null ? new String[0] : encoded.split("\\|", -1);
        org.bukkit.Location location = null;
        try {
            if (fields.length == 6) location = new org.bukkit.Location(new FotonWorld(fields[0]),
                Double.parseDouble(fields[1]), Double.parseDouble(fields[2]), Double.parseDouble(fields[3]),
                Float.parseFloat(fields[4]), Float.parseFloat(fields[5]));
        } catch (RuntimeException ignored) { }
        org.spigotmc.event.player.PlayerSpawnLocationEvent event =
            new org.spigotmc.event.player.PlayerSpawnLocationEvent(player(uuid), location);
        dispatch(event);
        org.bukkit.Location result = event.getSpawnLocation();
        if (result == null || result.getWorld() == null) return encoded;
        return result.getWorld().getName() + "|" + result.getX() + "|" + result.getY() + "|"
            + result.getZ() + "|" + result.getYaw() + "|" + result.getPitch();
    }

    public static void fireChunkLoad(String world, int x, int z, boolean newlyGenerated) {
        dispatch(new org.bukkit.event.world.ChunkLoadEvent(new FotonChunk(new FotonWorld(world), x, z), newlyGenerated));
    }

    public static String firePlayerPortal(String uuid, String encoded) {
        String[] f = encoded == null ? new String[0] : encoded.split("\\|", -1);
        if (f.length < 13) return "1|" + (f.length > 6 ? f[6] : "") + "|0|0|0|0|0";
        try {
            org.bukkit.Location from = new org.bukkit.Location(new FotonWorld(f[0]), Double.parseDouble(f[1]), Double.parseDouble(f[2]), Double.parseDouble(f[3]), Float.parseFloat(f[4]), Float.parseFloat(f[5]));
            org.bukkit.Location to = new org.bukkit.Location(new FotonWorld(f[6]), Double.parseDouble(f[7]), Double.parseDouble(f[8]), Double.parseDouble(f[9]), Float.parseFloat(f[10]), Float.parseFloat(f[11]));
            org.bukkit.event.player.PlayerPortalEvent event = new org.bukkit.event.player.PlayerPortalEvent(player(uuid), from, to, org.bukkit.event.player.PlayerTeleportEvent.TeleportCause.valueOf(f[12]));
            dispatch(event);
            if (event.isCancelled()) return "0|";
            org.bukkit.Location result=event.getTo(); if(result==null || result.getWorld()==null) return "1|"+f[6]+"|"+f[7]+"|"+f[8]+"|"+f[9]+"|"+f[10]+"|"+f[11];
            return "1|"+result.getWorld().getName()+"|"+result.getX()+"|"+result.getY()+"|"+result.getZ()+"|"+result.getYaw()+"|"+result.getPitch();
        } catch (RuntimeException ignored) { return "1|"+f[6]+"|"+f[7]+"|"+f[8]+"|"+f[9]+"|"+f[10]+"|"+f[11]; }
    }

    public static String firePortalCreate(String world, String encoded) {
        java.util.ArrayList<org.bukkit.block.BlockState> blocks = new java.util.ArrayList<>();
        if (encoded != null) for (String value : encoded.split(";", -1)) {
            String[] xyz = value.split(",", -1);
            if (xyz.length != 3) continue;
            try { blocks.add(new FotonBlock(new FotonWorld(world), Integer.parseInt(xyz[0]), Integer.parseInt(xyz[1]), Integer.parseInt(xyz[2])).getState()); }
            catch (RuntimeException ignored) { }
        }
        org.bukkit.event.world.PortalCreateEvent event =
            new org.bukkit.event.world.PortalCreateEvent(new FotonWorld(world), blocks);
        dispatch(event);
        if (event.isCancelled()) return "0|";
        StringBuilder result = new StringBuilder("1|");
        for (org.bukkit.block.BlockState block : event.getBlocks()) {
            if (block == null) continue;
            if (result.length() > 2) result.append(';');
            result.append(block.getX()).append(',').append(block.getY()).append(',').append(block.getZ());
        }
        return result.toString();
    }

    public static String firePlayerDeath(String uuid, String message, String encodedDrops, boolean keepInventory) {
        org.bukkit.event.entity.PlayerDeathEvent event =
            new org.bukkit.event.entity.PlayerDeathEvent(player(uuid), message, keepInventory);
        if (encodedDrops != null && !encodedDrops.isEmpty()) {
            for (String encoded : encodedDrops.split("\\u001e", -1)) {
                org.bukkit.inventory.ItemStack item = FotonInventory.decode(encoded);
                if (item != null) event.getDrops().add(item);
            }
        }
        dispatch(event);
        StringBuilder drops = new StringBuilder();
        for (org.bukkit.inventory.ItemStack item : event.getDrops()) {
            String encoded = FotonInventory.encode(item);
            if (encoded.isEmpty()) continue;
            if (drops.length() > 0) drops.append('\u001e');
            drops.append(encoded);
        }
        return (event.getDeathMessage() == null ? "" : event.getDeathMessage())
            + "\u001f" + drops;
    }

    public static void fireInventoryClose(String uuid) {
        FotonCustomInventory.detachViewer(uuid);
        dispatch(new org.bukkit.event.inventory.InventoryCloseEvent(player(uuid)));
    }

    public static boolean firePlayerOpenSign(String uuid, String world, int x, int y, int z, boolean front, String cause) {
        org.bukkit.block.Block block = new FotonBlock(new FotonWorld(world), x, y, z);
        org.bukkit.block.data.BlockData data = block.getBlockData();
        org.bukkit.block.Sign sign = new FotonSign(block, data);
        io.papermc.paper.event.player.PlayerOpenSignEvent event =
            new io.papermc.paper.event.player.PlayerOpenSignEvent(player(uuid), sign,
                front ? org.bukkit.block.sign.Side.FRONT : org.bukkit.block.sign.Side.BACK,
                io.papermc.paper.event.player.PlayerOpenSignEvent.Cause.valueOf(cause));
        dispatch(event);
        return !event.isCancelled();
    }

    public static String fireSignChange(String uuid, String world, int x, int y, int z, String encoded) {
        String[] lines = encoded == null ? new String[] {"", "", "", ""} : encoded.split("\\u001f", -1);
        if (lines.length != 4) lines = new String[] {"", "", "", ""};
        org.bukkit.event.block.SignChangeEvent event = new org.bukkit.event.block.SignChangeEvent(
            new FotonBlock(new FotonWorld(world), x, y, z), player(uuid), lines);
        dispatch(event);
        StringBuilder answer = new StringBuilder(event.isCancelled() ? "1" : "0");
        for (String line : event.getLines()) answer.append('\u001f').append(line == null ? "" : line);
        return answer.toString();
    }

    public static boolean fireWeatherChange(String world, boolean raining) {
        org.bukkit.event.weather.WeatherChangeEvent event = new org.bukkit.event.weather.WeatherChangeEvent(new FotonWorld(world), raining);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireThunderChange(String world, boolean thundering) {
        org.bukkit.event.weather.ThunderChangeEvent event = new org.bukkit.event.weather.ThunderChangeEvent(new FotonWorld(world), thundering);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireLightningStrike(String entityUuid, String world, String cause) {
        org.bukkit.entity.Entity raw = FotonEntity.handle(Native.parse(entityUuid));
        if (!(raw instanceof org.bukkit.entity.LightningStrike lightning)) return true;
        org.bukkit.World w = new FotonWorld(world);
        org.bukkit.event.weather.LightningStrikeEvent event = new org.bukkit.event.weather.LightningStrikeEvent(w, lightning, org.bukkit.event.weather.LightningStrikeEvent.Cause.valueOf(cause));
        dispatch(event);
        return !event.isCancelled();
    }

    public static String fireEntityExplode(String entityUuid, String world, String encoded, String result) {
        java.util.List<org.bukkit.block.Block> blocks = new java.util.ArrayList<>();
        if (encoded != null && !encoded.isEmpty()) {
            for (String value : encoded.split(";")) {
                String[] xyz = value.split(",");
                if (xyz.length != 3) continue;
                try {
                    blocks.add(new FotonBlock(new FotonWorld(world), Integer.parseInt(xyz[0].trim()),
                        Integer.parseInt(xyz[1].trim()), Integer.parseInt(xyz[2].trim())));
                } catch (NumberFormatException ignored) { }
            }
        }
        org.bukkit.event.entity.EntityExplodeEvent event = new org.bukkit.event.entity.EntityExplodeEvent(
            FotonEntity.handle(Native.parse(entityUuid)), blocks, 1.0f, org.bukkit.ExplosionResult.valueOf(result));
        dispatch(event);
        StringBuilder answer = new StringBuilder(event.isCancelled() ? "1" : "0").append('\u001f');
        for (org.bukkit.block.Block block : event.blockList()) {
            answer.append(block.getX()).append(',').append(block.getY()).append(',').append(block.getZ()).append(';');
        }
        return answer.toString();
    }

    public static String fireBlockExplode(String world, int x, int y, int z, String encoded) {
        java.util.List<org.bukkit.block.Block> blocks = new java.util.ArrayList<>();
        if (encoded != null && !encoded.isEmpty()) for (String value : encoded.split(";")) {
            String[] xyz = value.split(",");
            if (xyz.length != 3) continue;
            try { blocks.add(new FotonBlock(new FotonWorld(world), Integer.parseInt(xyz[0].trim()), Integer.parseInt(xyz[1].trim()), Integer.parseInt(xyz[2].trim()))); }
            catch (NumberFormatException ignored) { }
        }
        org.bukkit.event.block.BlockExplodeEvent event = new org.bukkit.event.block.BlockExplodeEvent(
            new FotonBlock(new FotonWorld(world), x, y, z), blocks, 1.0f);
        dispatch(event);
        StringBuilder answer = new StringBuilder(event.isCancelled() ? "1" : "0").append('\u001f');
        for (org.bukkit.block.Block block : event.blockList()) answer.append(block.getX()).append(',').append(block.getY()).append(',').append(block.getZ()).append(';');
        return answer.toString();
    }

    public static String fireBlockDispense(String world, int x, int y, int z, String item) {
        org.bukkit.event.block.BlockDispenseEvent event = new org.bukkit.event.block.BlockDispenseEvent(
            new FotonBlock(new FotonWorld(world), x, y, z), FotonInventory.decode(item));
        dispatch(event);
        return (event.isCancelled() ? "1" : "0") + '\u001f' + FotonInventory.encode(event.getItem());
    }

    public static String fireBlockPreDispense(String world, int x, int y, int z, int slot, String item) {
        io.papermc.paper.event.block.BlockPreDispenseEvent event =
            new io.papermc.paper.event.block.BlockPreDispenseEvent(
                new FotonBlock(new FotonWorld(world), x, y, z), slot, FotonInventory.decode(item));
        dispatch(event);
        return (event.isCancelled() ? "1" : "0") + '\u001f' + FotonInventory.encode(event.getItem());
    }

    public static boolean fireEntityTransform(String entityUuid, String transformedUuid, String reason) {
        org.bukkit.event.entity.EntityTransformEvent event = new org.bukkit.event.entity.EntityTransformEvent(
            FotonEntity.handle(Native.parse(entityUuid)), FotonEntity.handle(Native.parse(transformedUuid)),
            org.bukkit.event.entity.EntityTransformEvent.TransformReason.valueOf(reason));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityChangeBlock(String entityUuid, String world, int x, int y, int z, String to) {
        org.bukkit.Material material = org.bukkit.Material.matchMaterial(to);
        if (material == null) material = org.bukkit.Material.AIR;
        org.bukkit.event.entity.EntityChangeBlockEvent event = new org.bukkit.event.entity.EntityChangeBlockEvent(
            FotonEntity.handle(Native.parse(entityUuid)), new FotonBlock(new FotonWorld(world), x, y, z), material);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireBlockDamage(String playerUuid, String world, int x, int y, int z) {
        org.bukkit.event.block.BlockDamageEvent event = new org.bukkit.event.block.BlockDamageEvent(
            player(playerUuid), new FotonBlock(new FotonWorld(world), x, y, z));
        dispatch(event); return !event.isCancelled();
    }

    public static boolean firePlayerAdvancementCriterionGrant(String playerUuid, String key, String criterion) {
        org.bukkit.NamespacedKey namespaced = org.bukkit.NamespacedKey.fromString(key);
        if (namespaced == null) return false;
        com.destroystokyo.paper.event.player.PlayerAdvancementCriterionGrantEvent event =
            new com.destroystokyo.paper.event.player.PlayerAdvancementCriterionGrantEvent(
                player(playerUuid),
                new FotonAdvancement(namespaced, Native.advancementCriteria(key)),
                criterion);
        dispatch(event);
        return !event.isCancelled();
    }

    public static void firePlayerAdvancementDone(String playerUuid, String key) {
        org.bukkit.NamespacedKey namespaced = org.bukkit.NamespacedKey.fromString(key);
        if (namespaced == null) return;
        dispatch(new org.bukkit.event.player.PlayerAdvancementDoneEvent(
            player(playerUuid), new FotonAdvancement(namespaced, Native.advancementCriteria(key))));
    }

    public static String firePiston(String world, int x, int y, int z, String direction,
            boolean extending, String encoded) {
        java.util.List<org.bukkit.block.Block> blocks = new java.util.ArrayList<>();
        if (encoded != null && !encoded.isEmpty()) for (String value : encoded.split(";")) {
            String[] xyz = value.split(","); if (xyz.length != 3) continue;
            try { blocks.add(new FotonBlock(new FotonWorld(world), Integer.parseInt(xyz[0].trim()),
                Integer.parseInt(xyz[1].trim()), Integer.parseInt(xyz[2].trim()))); }
            catch (NumberFormatException ignored) { }
        }
        org.bukkit.block.BlockFace face;
        try { face = org.bukkit.block.BlockFace.valueOf(direction.toUpperCase(java.util.Locale.ROOT)); }
        catch (IllegalArgumentException error) { face = org.bukkit.block.BlockFace.SELF; }
        org.bukkit.event.block.PistonEvent event = extending
            ? new org.bukkit.event.block.BlockPistonExtendEvent(new FotonBlock(new FotonWorld(world), x, y, z), face, blocks)
            : new org.bukkit.event.block.BlockPistonRetractEvent(new FotonBlock(new FotonWorld(world), x, y, z), face, blocks);
        dispatch(event);
        StringBuilder answer = new StringBuilder(event.isCancelled() ? "1" : "0").append('\u001f');
        for (org.bukkit.block.Block block : event.getBlocks()) answer.append(block.getX()).append(',')
            .append(block.getY()).append(',').append(block.getZ()).append(';');
        return answer.toString();
    }

    public static String fireMove(String uuid, String world,
            double fromX, double fromY, double fromZ,
            double toX, double toY, double toZ) {
        PlayerMoveEvent event = new PlayerMoveEvent(player(uuid),
            new org.bukkit.Location(new FotonWorld(world), fromX, fromY, fromZ),
            new org.bukkit.Location(new FotonWorld(world), toX, toY, toZ));
        dispatch(event);
        if (event.isCancelled()) return null;
        Location to = event.getTo();
        if (to == null) return null;
        if (to.getX() == toX && to.getY() == toY && to.getZ() == toZ) return "";
        return to.getX() + "," + to.getY() + "," + to.getZ();
    }

    /** How many handlers are registered for one event type. For diagnostics. */
    public static int handlerCount(String className) {
        for (Map.Entry<Class<?>, List<Handler>> entry : handlers.entrySet()) {
            if (entry.getKey().getName().equals(className)) {
                return entry.getValue().size();
            }
        }
        return 0;
    }

    /** One handler, however the plugin gave it to us.
     *
     * An annotated method and a hand-registered executor are the same thing to
     * everyone downstream, so they are the same thing here: `call` is what
     * dispatch uses and `name` is what a log line says.
     */
    private static final class Handler {
        final Listener listener;
        final Method method;
        final EventExecutor executor;
        final EventPriority priority;
        final boolean ignoreCancelled;
        final Plugin plugin;

        Handler(Listener listener, Method method, EventPriority priority,
                boolean ignoreCancelled, Plugin plugin) {
            this(listener, method, null, priority, ignoreCancelled, plugin);
        }

        Handler(Listener listener, Method method, EventExecutor executor, EventPriority priority,
                boolean ignoreCancelled, Plugin plugin) {
            this.listener = listener;
            this.method = method;
            this.executor = executor;
            this.priority = priority;
            this.ignoreCancelled = ignoreCancelled;
            this.plugin = plugin;
        }

        void call(Object event) throws Throwable {
            if (executor != null) {
                executor.execute(listener, (org.bukkit.event.Event) event);
            } else {
                method.invoke(listener, event);
            }
        }

        String name() {
            return method == null ? "a registered handler" : method.getName();
        }
    }

    /** Offers a typed command to the plugins. True means one owned it.
     *
     * False is the important answer: it has to mean "nobody claimed this",
     * because Foton takes it as permission to go on to its own dispatcher. A
     * handler that ran and failed still answers true.
     */
    public static boolean fireCommand(String uuid, String line) {
        CommandSender sender = uuid == null || uuid.isEmpty()
            ? ConsoleSender.INSTANCE
            : new FotonPlayer(java.util.UUID.fromString(uuid));
        try {
            return CommandMap.dispatch(sender, line);
        } catch (Throwable error) {
            System.out.println("[command] dispatch failed: " + error);
            return false;
        }
    }

}
