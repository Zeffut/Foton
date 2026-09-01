package foton;

import java.util.UUID;

/** Everything the Java side asks Foton for.
 *
 * One class, all static, all native. The Rust side registers these with
 * `RegisterNatives` when it starts the runtime rather than relying on symbol
 * lookup, because Foton is a single binary and its symbols are its own
 * business.
 *
 * Players are named by UUID rather than by a pointer. A pointer would have to
 * stay valid for as long as a plugin kept the object, and plugins keep them for
 * a long time; a UUID that no longer resolves is a player who logged out, which
 * is a thing Bukkit's own API already has an answer for.
 */
public final class Native {
    private Native() {}

    public static native String serverName();
    public static native String serverVersion();

    /** The UUIDs of everyone online, in no promised order. */
    public static native String[] onlinePlayerIds();

    /** A player's name, or null once they are gone. */
    public static native String playerName(String uuid);

    /** Sends a player a chat message. Silently does nothing once they are gone. */
    public static native void sendMessage(String uuid, String message);

    /** The name of the world a player is in, or null once they are gone. */
    public static native String playerWorld(String uuid);

    /** Whether a player holds a permission. */
    public static native boolean hasPermission(String uuid, String permission);

    static UUID parse(String uuid) {
        try {
            return UUID.fromString(uuid);
        } catch (IllegalArgumentException error) {
            return null;
        }
    }
}
