package io.papermc.paper.network;

import io.netty.channel.Channel;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import net.kyori.adventure.key.Key;

/**
 * Registry for Paper-compatible channel initialization listeners.
 *
 * <p>The public method descriptors intentionally match Paper's internal API.
 */
public final class ChannelInitializeListenerHolder {
    private static final Map<Key, ChannelInitializeListener> LISTENERS =
        new LinkedHashMap<>();
    private static final Map<Key, ChannelInitializeListener> IMMUTABLE_VIEW =
        Collections.unmodifiableMap(LISTENERS);

    private ChannelInitializeListenerHolder() {}

    public static boolean hasListener(Key key) {
        return LISTENERS.containsKey(key);
    }

    public static void addListener(Key key, ChannelInitializeListener listener) {
        LISTENERS.put(key, listener);
    }

    public static ChannelInitializeListener removeListener(Key key) {
        return LISTENERS.remove(key);
    }

    public static Map<Key, ChannelInitializeListener> getListeners() {
        return IMMUTABLE_VIEW;
    }

    public static void callListeners(Channel channel) {
        for (ChannelInitializeListener listener : LISTENERS.values()) {
            listener.afterInitChannel(channel);
        }
    }
}
