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

    /** Whether this call is running on Foton's serialized game-tick thread. */
    public static native boolean isPrimaryThread();

    /** Requests the server's normal graceful shutdown sequence. */
    public static native void shutdown();

    /** The UUIDs of everyone online, in no promised order. */
    public static native String[] onlinePlayerIds();
    public static native String[] worldPlayerIds(String world);
    public static native String[] worldEntityIds(String world);
    public static native boolean worldChunkLoaded(String world, int x, int z);
    public static native String[] worldLoadedChunkCoords(String world);
    public static native String worldFolder(String world);
    public static native String worldDropItem(String world, double x, double y, double z, String item);
    public static native String[] scoreboardTeamEntries(String world, String team);
    public static native String scoreboardEntryTeam(String world, String entry);

    /** A player's name, or null once they are gone. */
    public static native String playerName(String uuid);
    public static native String playerLocale(String uuid);
    public static native boolean hasPlayedBefore(String uuid);
    public static native String customName(String uuid);
    public static native void setCustomName(String uuid, String name);
    public static native double health(String uuid);
    public static native void setHealth(String uuid, double health);
    public static native double maxHealth(String uuid);
    public static native int experienceLevel(String uuid);

    /** The UUID of an online player with this name, or null. */
    public static native String playerIdByName(String name);

    /** Sends a player a chat message. Silently does nothing once they are gone. */
    public static native void sendMessage(String uuid, String message);

    /** Disconnects an online player with the supplied message. */
    public static native void kickPlayer(String uuid, String message);

    public static native void setPlayerListHeader(String uuid, String header);
    public static native void setPlayerListFooter(String uuid, String footer);
    public static native void setPlayerListHeaderFooter(String uuid, String header, String footer);

    public static native void sendActionBar(String uuid, String message);
    public static native void sendTitle(String uuid, String title, String subtitle,
            int fadeIn, int stay, int fadeOut);
    public static native void clearTitle(String uuid);

    /** Sends one custom payload packet to one online player. */
    public static native void sendPluginMessage(String uuid, String channel, byte[] message);

    /** Sends everyone a chat message, and says how many heard it. */
    public static native int broadcast(String message);

    /** The name of the world a player is in, or null once they are gone. */
    public static native String playerWorld(String uuid);
    public static native String entityWorld(String uuid);
    public static native String entityType(String uuid);
    public static native double[] entityPosition(String uuid);
    public static native int entityId(String uuid);
    public static native String entityCustomName(String uuid);
    public static native void setEntityCustomName(String uuid, String name);
    public static native void entitySendMessage(String uuid, String message);

    /** Whether a player holds a permission. */
    public static native boolean hasPermission(String uuid, String permission);
    public static native boolean isPermissionSet(String uuid, String permission);
    public static native boolean fireInventoryClick(String uuid, String item);

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
    public static native boolean isSneaking(String uuid);
    public static native void openBook(String uuid);
    public static native boolean teleport(String uuid, String world, double x, double y,
            double z, float yaw, float pitch);

    /** Creates a native boss event and returns its opaque UUID handle. */
    public static native String createBossBar(String title, int color, int style, int flags);

    public static native void releaseBossBar(String id);
    public static native void bossBarSetTitle(String id, String title);
    public static native void bossBarSetColor(String id, int color);
    public static native void bossBarSetStyle(String id, int style);
    public static native void bossBarSetFlags(String id, int flags);
    public static native void bossBarSetProgress(String id, double progress);
    public static native void bossBarAddPlayer(String id, String player);
    public static native void bossBarRemovePlayer(String id, String player);
    public static native void bossBarRemoveAll(String id);
    public static native String[] bossBarPlayerIds(String id);
    public static native void bossBarSetVisible(String id, boolean visible);


    /** Plays a sound at a point in a world, for everyone who can hear it. */
    public static native void playSound(
        String world, double x, double y, double z, String sound, float volume, float pitch);
    public static native void playSoundCategory(
        String world, double x, double y, double z, String sound, String category, float volume, float pitch);
    public static native void stopSound(String uuid, String sound, String category);

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
    public static native int worldMinHeight(String world);
    public static native int worldMaxHeight(String world);

    static UUID parse(String uuid) {
        try {
            return UUID.fromString(uuid);
        } catch (IllegalArgumentException error) {
            return null;
        }
    }
}
