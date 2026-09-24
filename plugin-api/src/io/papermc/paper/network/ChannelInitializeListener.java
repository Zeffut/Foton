package io.papermc.paper.network;

import io.netty.channel.Channel;

/**
 * Internal Paper-compatible hook called after a network channel is initialized.
 *
 * <p>Paper does not guarantee this internal API, but ViaVersion uses its binary
 * signature to install protocol handlers without depending on CraftBukkit internals.
 */
@FunctionalInterface
public interface ChannelInitializeListener {
    void afterInitChannel(Channel channel);
}
