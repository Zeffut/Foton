package foton.packetevents;

import com.github.retrooper.packetevents.netty.buffer.ByteBufHelper;
import foton.Native;
import com.github.retrooper.packetevents.netty.buffer.UnpooledByteBufAllocationHelper;
import com.github.retrooper.packetevents.netty.channel.ChannelOperator;
import java.net.InetSocketAddress;
import java.net.SocketAddress;
import java.util.List;
import org.bukkit.Bukkit;

/** The channel operations PacketEvents needs, answered by Foton.
 *
 * A buffer handed to a write holds a packet id and its payload, which is
 * exactly what the tap carries. "In context" is how PacketEvents asks for a
 * write or a read that skips its own handlers: here that is the silent flag,
 * which keeps the packet away from the tap and so from every listener.
 *
 * Writing and flushing are one step: Foton queues a whole packet and its
 * network task writes it, so there is nothing a separate flush would push.
 */
final class FotonChannelOperator implements ChannelOperator {
    @Override public SocketAddress remoteAddress(Object channel) { return ((FotonChannel) channel).address; }

    @Override public SocketAddress localAddress(Object channel) {
        return new InetSocketAddress(Bukkit.getIp().isEmpty() ? "0.0.0.0" : Bukkit.getIp(), Bukkit.getPort());
    }

    @Override public boolean isOpen(Object channel) { return ((FotonChannel) channel).open; }

    @Override public Object close(Object channel) {
        FotonChannel target = (FotonChannel) channel;
        if (target.player != null) target.player.kickPlayer("");
        return null;
    }

    @Override public Object write(Object channel, Object buffer) { return send(channel, buffer, false); }
    @Override public Object flush(Object channel) { return null; }
    @Override public Object writeAndFlush(Object channel, Object buffer) { return send(channel, buffer, false); }
    @Override public Object fireChannelRead(Object channel, Object buffer) { return receive(channel, buffer, false); }
    @Override public Object writeInContext(Object channel, String ctx, Object buffer) { return send(channel, buffer, true); }
    @Override public Object flushInContext(Object channel, String ctx) { return null; }
    @Override public Object writeAndFlushInContext(Object channel, String ctx, Object buffer) { return send(channel, buffer, true); }
    @Override public Object fireChannelReadInContext(Object channel, String ctx, Object buffer) { return receive(channel, buffer, true); }

    @Override public List<String> pipelineHandlerNames(Object channel) { return List.of(); }
    @Override public Object getPipelineHandler(Object channel, String name) { return null; }
    @Override public Object getPipelineContext(Object channel, String name) { return null; }
    /** The channel is its own pipeline: it is what PacketEvents keys users by. */
    @Override public Object getPipeline(Object channel) { return channel; }

    /** There is no event loop to hand work to; the caller's thread is as good as any. */
    @Override public void runInEventLoop(Object channel, Runnable runnable) { runnable.run(); }

    @Override public Object pooledByteBuf(Object channel) { return UnpooledByteBufAllocationHelper.buffer(); }

    private static Object send(Object channel, Object buffer, boolean silent) {
        FotonChannel target = (FotonChannel) channel;
        try {
            int id = ByteBufHelper.readVarInt(buffer);
            byte[] payload = new byte[ByteBufHelper.readableBytes(buffer)];
            ByteBufHelper.readBytes(buffer, payload);
            Native.packetSend(target.uuid.toString(), id, payload, silent);
        } finally {
            ByteBufHelper.release(buffer);
        }
        return null;
    }

    private static Object receive(Object channel, Object buffer, boolean silent) {
        FotonChannel target = (FotonChannel) channel;
        try {
            int id = ByteBufHelper.readVarInt(buffer);
            byte[] payload = new byte[ByteBufHelper.readableBytes(buffer)];
            ByteBufHelper.readBytes(buffer, payload);
            Native.packetReceive(target.uuid.toString(), id, payload, silent);
        } finally {
            ByteBufHelper.release(buffer);
        }
        return null;
    }
}
