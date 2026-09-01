package foton;

import org.bukkit.command.CommandSender;

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
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;
import org.bukkit.plugin.Plugin;

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

    /** Runs every handler for one event, in priority order. */
    private static void dispatch(Object event) {
        List<Handler> list = handlers.get(event.getClass());
        if (list == null) {
            return;
        }
        boolean cancellable = event instanceof Cancellable;
        for (Handler handler : List.copyOf(list)) {
            if (cancellable && ((Cancellable) event).isCancelled() && !handler.ignoreCancelled) {
                continue;
            }
            try {
                handler.method.invoke(handler.listener, event);
            } catch (Throwable error) {
                // One plugin throwing must not stop the others, and must not
                // reach Foton: an exception crossing JNI is a crash, not an
                // error message.
                System.out.println("[events] " + handler.plugin.getName() + " threw in "
                    + handler.method.getName() + ": " + rootOf(error));
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

    /** A player left. Returns what to announce, or null to announce nothing. */
    public static String fireQuit(String uuid, String message) {
        PlayerQuitEvent event = new PlayerQuitEvent(player(uuid), message);
        dispatch(event);
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

    /** How many handlers are registered for one event type. For diagnostics. */
    public static int handlerCount(String className) {
        for (Map.Entry<Class<?>, List<Handler>> entry : handlers.entrySet()) {
            if (entry.getKey().getName().equals(className)) {
                return entry.getValue().size();
            }
        }
        return 0;
    }

    private static final class Handler {
        final Listener listener;
        final Method method;
        final EventPriority priority;
        final boolean ignoreCancelled;
        final Plugin plugin;

        Handler(Listener listener, Method method, EventPriority priority,
                boolean ignoreCancelled, Plugin plugin) {
            this.listener = listener;
            this.method = method;
            this.priority = priority;
            this.ignoreCancelled = ignoreCancelled;
            this.plugin = plugin;
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
