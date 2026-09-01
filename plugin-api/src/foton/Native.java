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
 *
 * A position crosses as five doubles in one array rather than five calls. Five
 * calls could each see a different tick, and a plugin that read x from one tick
 * and z from the next would get a point the player was never at.
 */
public final class Native {
    private Native() {}

    public static native String serverName();

    public static native String serverVersion();

    /** How the server describes itself, the way `/version` prints it. */
    public static native String serverBrand();

    public static native boolean onlineMode();

    public static native int maxPlayers();

    /** The UUIDs of everyone online, in no promised order. */
    public static native String[] onlinePlayerIds();

    /** A player's name, or null once they are gone. */
    public static native String playerName(String uuid);

    /** The UUID of an online player with this name, or null. */
    public static native String playerIdByName(String name);

    /** Sends a player a chat message. Silently does nothing once they are gone. */
    public static native void sendMessage(String uuid, String message);

    /** Sends everyone a chat message, and says how many heard it. */
    public static native int broadcast(String message);

    /** The name of the world a player is in, or null once they are gone. */
    public static native String playerWorld(String uuid);

    /** Whether a player holds a permission. */
    public static native boolean hasPermission(String uuid, String permission);

    /** A player's position as {x, y, z, yaw, pitch}, or null once they are gone. */
    public static native double[] playerPosition(String uuid);

    /** A player's game mode, lower case, or null once they are gone. */
    public static native String gameMode(String uuid);

    /** One inventory slot as `minecraft:name count`, or the empty string.
     *
     * A string rather than an object: building a Java object from Rust means
     * naming a constructor by signature, and a signature that drifts is a
     * NoSuchMethodError at the worst possible moment.
     */
    public static native String inventorySlot(String uuid, int slot);

    /** Writes one inventory slot. An empty string empties it. */
    public static native void setInventorySlot(String uuid, int slot, String item);

    /** Which hotbar slot a player is holding, or -1 once they are gone. */
    public static native int heldSlot(String uuid);

    /** Whether a player is an operator. */
    public static native boolean isOperator(String uuid);

    /** Plays a sound at a point in a world, for everyone who can hear it. */
    public static native void playSound(
        String world, double x, double y, double z, String sound, float volume, float pitch);

    /** One block as `minecraft:name[state=value]`, or null if unreadable. */
    public static native String blockState(String world, int x, int y, int z);

    /** Writes one block from the same text. */
    public static native void setBlock(String world, int x, int y, int z, String state);

    /** Every loaded world's key, in no promised order. */
    public static native String[] worldNames();

    /** A world's spawn as {x, y, z, yaw, pitch}, or null if there is no such world. */
    public static native double[] worldSpawn(String world);

    /** A world's time of day, or -1 if there is no such world. */
    public static native long worldTime(String world);

    static UUID parse(String uuid) {
        try {
            return UUID.fromString(uuid);
        } catch (IllegalArgumentException error) {
            return null;
        }
    }
}
