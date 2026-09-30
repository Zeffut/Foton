package foton;

/** Where Foton hands raw packets to Java, and where it reads back the answer.
 *
 * A packet library -- PacketEvents is the one that matters -- installs one
 * {@link Handler} and turns the tap on with {@link #install}. From then on
 * every packet a client sends is shown to it before Foton handles it, and
 * every packet Foton sends is shown to it before it is written.
 *
 * The calls come from Foton's network threads, never from the tick thread,
 * which is where a Netty server calls the same libraries. A handler must be
 * quick and must not block: a slow handler is a slow connection.
 *
 * The answer is an int so one crossing carries it: {@link #PASS},
 * {@link #CANCEL} or {@link #REWRITE} in the low bits, with {@link #AFTER_SEND}
 * set when work waits for the packet to be written. A rewrite's bytes are
 * collected by Foton with {@link #takeRewrite} on the same thread, straight
 * after.
 */
public final class PacketBridge {
    public static final int PASS = 0;
    public static final int CANCEL = 1;
    public static final int REWRITE = 2;
    public static final int AFTER_SEND = 1 << 4;

    /** The phase a packet belongs to; ids are only unique within one. */
    public static final int CONFIGURATION = 0;
    public static final int PLAY = 1;

    /** What a packet library implements. */
    public interface Handler {
        void opened(long connection, java.util.UUID profile, String name, String address);
        void playing(long connection, java.util.UUID player, int entityId);
        void closed(long connection);
        /** Returns PASS, CANCEL, or REWRITE after calling {@link PacketBridge#rewrite}. */
        int inbound(long connection, int phase, int packetId, byte[] payload);
        /** As {@link #inbound}, plus {@link PacketBridge#AFTER_SEND} when needed. */
        int outbound(long connection, int phase, int packetId, byte[] payload);
        /** A packet whose answer carried AFTER_SEND has been written. */
        void sent(long connection);
    }

    private static volatile Handler handler;
    private static final ThreadLocal<byte[]> REWRITTEN = new ThreadLocal<>();

    private PacketBridge() {}

    /** Installs a handler and turns the tap on. Returns false if Foton refused. */
    public static synchronized boolean install(Handler installed) {
        handler = installed;
        if (Native.packetTapEnable(installed != null)) {
            return true;
        }
        handler = null;
        return false;
    }

    /** Turns the tap off, if this handler is still the one installed. */
    public static synchronized void uninstall(Handler installed) {
        if (handler != installed) {
            return;
        }
        Native.packetTapEnable(false);
        handler = null;
    }

    /** Records the payload a REWRITE answer stands for. */
    public static void rewrite(byte[] payload) {
        REWRITTEN.set(payload);
    }

    // Called by Foton.

    static void opened(long connection, String profile, String name, String address) {
        Handler current = handler;
        if (current != null) current.opened(connection, java.util.UUID.fromString(profile), name, address);
    }

    static void playing(long connection, String player, int entityId) {
        Handler current = handler;
        if (current != null) current.playing(connection, java.util.UUID.fromString(player), entityId);
    }

    static void closed(long connection) {
        Handler current = handler;
        if (current != null) current.closed(connection);
    }

    static int inbound(long connection, int phase, int packetId, byte[] payload) {
        Handler current = handler;
        return current == null ? PASS : current.inbound(connection, phase, packetId, payload);
    }

    static int outbound(long connection, int phase, int packetId, byte[] payload) {
        Handler current = handler;
        return current == null ? PASS : current.outbound(connection, phase, packetId, payload);
    }

    static void sent(long connection) {
        Handler current = handler;
        if (current != null) current.sent(connection);
    }

    static byte[] takeRewrite() {
        byte[] payload = REWRITTEN.get();
        REWRITTEN.remove();
        return payload;
    }
}
