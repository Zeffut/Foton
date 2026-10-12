package foton.network;

import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/** Entry point used through JNI to create one Via pipeline per TCP connection. */
public final class FotonViaBridge {
    private static final ConcurrentHashMap<UUID, FotonViaChannel> BOUND = new ConcurrentHashMap<>();

    private FotonViaBridge() {}

    /** Returns null when no plugin registered a Paper channel listener. */
    public static FotonViaChannel open() {
        if (io.papermc.paper.network.ChannelInitializeListenerHolder.getListeners().isEmpty()) {
            return null;
        }
        return new FotonViaChannel();
    }

    /** Called once a connection's player is known, before any plugin hears the player join. */
    public static void bind(String player, FotonViaChannel channel) {
        UUID id = UUID.fromString(player);
        channel.bindTo(id);
        BOUND.put(id, channel);
    }

    /** Forgets a closed channel, unless the player has already reconnected on another one. */
    static void unbind(UUID player, FotonViaChannel channel) {
        BOUND.remove(player, channel);
    }

    /** The Netty channel Via injected into this player's connection, or null. */
    public static io.netty.channel.Channel channelOf(UUID player) {
        FotonViaChannel channel = BOUND.get(player);
        return channel == null ? null : channel.nettyChannel();
    }
}
