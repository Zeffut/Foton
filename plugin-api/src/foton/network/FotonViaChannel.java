package foton.network;

import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInboundHandlerAdapter;
import io.netty.channel.ChannelOutboundHandlerAdapter;
import io.netty.channel.ChannelPromise;
import io.netty.channel.embedded.EmbeddedChannel;
import io.netty.util.ReferenceCountUtil;
import java.util.ArrayList;
import java.util.List;

/** A Paper-shaped Netty channel used by Foton's Rust network transport. */
public final class FotonViaChannel implements AutoCloseable {
    private static final int SERVERBOUND = 0;
    private static final int CLIENTBOUND = 1;
    private static final int MAX_OUTPUT_PACKETS = 256;
    private static final int MAX_OUTPUT_BYTES = 32 * 1024 * 1024;

    private final EmbeddedChannel channel;
    private boolean closed;

    FotonViaChannel() {
        channel = new EmbeddedChannel();
        try {
            channel.pipeline().addLast("decoder", new IdentityInboundHandler());
            channel.pipeline().addLast("encoder", new IdentityOutboundHandler());
            io.papermc.paper.network.ChannelInitializeListenerHolder.callListeners(channel);
            channel.checkException();
        } catch (Throwable error) {
            channel.finishAndReleaseAll();
            throw error;
        }
    }

    /** Runs one unframed, decompressed serverbound Minecraft packet through Via. */
    public synchronized byte[][] serverbound(byte[] packet) {
        requireOpen();
        ByteBuf input = channel.alloc().buffer(packet.length);
        try {
            input.writeBytes(packet);
            ByteBuf transferred = input;
            input = null;
            try {
                channel.writeInbound(transferred);
            } catch (Throwable error) {
                rethrowUnlessCancelled(error);
            }
            return runAndDrain();
        } finally {
            ReferenceCountUtil.release(input);
        }
    }

    /** Runs one unframed, decompressed clientbound Minecraft packet through Via. */
    public synchronized byte[][] clientbound(byte[] packet) {
        requireOpen();
        ByteBuf input = channel.alloc().buffer(packet.length);
        try {
            input.writeBytes(packet);
            ByteBuf transferred = input;
            input = null;
            try {
                channel.writeOutbound(transferred);
            } catch (Throwable error) {
                rethrowUnlessCancelled(error);
            }
            return runAndDrain();
        } finally {
            ReferenceCountUtil.release(input);
        }
    }

    /** Drains packets emitted by Via's scheduled tasks without injecting input. */
    public synchronized byte[][] poll() {
        requireOpen();
        return runAndDrain();
    }

    private byte[][] runAndDrain() {
        try {
            channel.runPendingTasks();
            channel.runScheduledPendingTasks();
            channel.checkException();
        } catch (Throwable error) {
            rethrowUnlessCancelled(error);
        }

        List<byte[]> output = new ArrayList<>();
        int totalBytes = 0;
        totalBytes = drain(output, SERVERBOUND, true, totalBytes);
        drain(output, CLIENTBOUND, false, totalBytes);
        return output.toArray(new byte[0][]);
    }

    private int drain(List<byte[]> output, int direction, boolean inbound, int totalBytes) {
        Object message;
        while ((message = inbound ? channel.readInbound() : channel.readOutbound()) != null) {
            try {
                if (!(message instanceof ByteBuf buffer)) {
                    throw new IllegalStateException(
                        "Via emitted an unsupported message type: " + message.getClass().getName());
                }
                int length = buffer.readableBytes();
                if (output.size() >= MAX_OUTPUT_PACKETS
                    || length > MAX_OUTPUT_BYTES - totalBytes) {
                    throw new IllegalStateException("Via output exceeded Foton's per-exchange limit");
                }
                byte[] marked = new byte[length + 1];
                marked[0] = (byte) direction;
                buffer.getBytes(buffer.readerIndex(), marked, 1, length);
                output.add(marked);
                totalBytes += length;
            } finally {
                ReferenceCountUtil.release(message);
            }
        }
        return totalBytes;
    }

    private void requireOpen() {
        if (closed) throw new IllegalStateException("Via channel is closed");
    }

    private static void rethrowUnlessCancelled(Throwable error) {
        Throwable current = error;
        for (int causeDepth = 0; current != null && causeDepth < 16; causeDepth++) {
            Class<?> type = current.getClass();
            while (type != null) {
                String name = type.getName();
                if (name.equals("com.viaversion.viaversion.exception.CancelEncoderException")
                    || name.equals("com.viaversion.viaversion.exception.CancelDecoderException")) {
                    return;
                }
                type = type.getSuperclass();
            }
            current = current.getCause();
        }
        FotonViaChannel.<RuntimeException>throwUnchecked(error);
    }

    @SuppressWarnings("unchecked")
    private static <T extends Throwable> void throwUnchecked(Throwable error) throws T {
        throw (T) error;
    }

    @Override
    public synchronized void close() {
        if (closed) return;
        closed = true;
        channel.finishAndReleaseAll();
    }

    private static final class IdentityInboundHandler extends ChannelInboundHandlerAdapter {
        @Override
        public void channelRead(ChannelHandlerContext context, Object message) {
            context.fireChannelRead(message);
        }
    }

    private static final class IdentityOutboundHandler extends ChannelOutboundHandlerAdapter {
        @Override
        public void write(ChannelHandlerContext context, Object message, ChannelPromise promise) {
            context.write(message, promise);
        }
    }
}
