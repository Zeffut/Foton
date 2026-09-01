package foton;

import org.bukkit.command.CommandSender;
import org.bukkit.Location;

import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.bukkit.entity.Player;
import org.bukkit.event.Cancellable;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.event.entity.EntityPickupItemEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerLoginEvent;
import org.bukkit.event.player.PlayerMoveEvent;
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
        List<Handler> list = handlers.get(event.getClass());
        if (list == null) {
            return;
        }
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

    /** A player is attempting an item interaction. */
    public static boolean fireInteract(String uuid) {
        org.bukkit.event.player.PlayerInteractEvent event =
            new org.bukkit.event.player.PlayerInteractEvent(player(uuid),
                org.bukkit.event.block.Action.RIGHT_CLICK_AIR,
                null, null, null);
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireInventoryClick(String uuid, String item) {
        org.bukkit.event.inventory.InventoryClickEvent event =
            new org.bukkit.event.inventory.InventoryClickEvent(player(uuid), FotonInventory.decode(item));
        dispatch(event);
        return !event.isCancelled();
    }

    public static void fireEntityRemove(String entity) {
        org.bukkit.entity.Entity handle = new FotonEntity(Native.parse(entity));
        dispatch(new org.bukkit.event.entity.EntityRemoveFromWorldEvent(handle));
        dispatch(new com.destroystokyo.paper.event.entity.EntityRemoveFromWorldEvent(handle));
    }

    public static boolean fireEntityPickup(String entity, String item) {
        org.bukkit.entity.LivingEntity living = new FotonLivingEntity(Native.parse(entity));
        EntityPickupItemEvent event = new EntityPickupItemEvent(living, new FotonItem(Native.parse(item)));
        dispatch(event);
        return !event.isCancelled();
    }

    public static boolean fireEntityDamage(String damager, String entity) {
        org.bukkit.event.entity.EntityDamageByEntityEvent event =
            new org.bukkit.event.entity.EntityDamageByEntityEvent(
                new FotonEntity(Native.parse(damager)), new FotonEntity(Native.parse(entity)));
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
    public static String fireChat(String uuid, String message) {
        AsyncPlayerChatEvent event = new AsyncPlayerChatEvent(player(uuid), message);
        dispatch(event);
        return event.isCancelled() ? null : event.getMessage();
    }

    /** A player is breaking a block. Returns false when a plugin stopped it. */
    public static boolean fireBlockBreak(String uuid, int x, int y, int z, String world) {
        BlockBreakEvent event =
            new BlockBreakEvent(new FotonBlock(new FotonWorld(world), x, y, z), player(uuid));
        dispatch(event);
        return !event.isCancelled();
    }

    /** A player is placing a block. Returns false when a plugin stopped it. */
    public static boolean fireBlockPlace(String uuid, int x, int y, int z, String world) {
        BlockPlaceEvent event =
            new BlockPlaceEvent(new FotonBlock(new FotonWorld(world), x, y, z), player(uuid));
        dispatch(event);
        return !event.isCancelled();
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
