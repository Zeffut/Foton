import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInboundHandlerAdapter;

/**
 * Re-injection toward the server must find a handler in front of Via's decoder (issue #44).
 *
 * <p>ViaVersion 5.12's server-side {@code sendToServer} asks {@code PipelineUtil.getPreviousContext}
 * for the context just before its decoder and fires the packet from there. It walks the pipeline's
 * names and passes the previous one to {@code pipeline.context(name)}; when Via's decoder is the
 * first handler that name is null and the call throws, which disconnects the client. Paper's
 * pipeline always has the frame splitter ahead of Via. This check installs a decoder the way
 * Via's channel initializer does, repeats that lookup, and sends a packet down the path.
 */
public class ViaPipelineCheck {
    public static void main(String[] args) throws Exception {
        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(
            net.kyori.adventure.key.Key.key("foton", "via_pipeline_check"),
            channel -> channel.pipeline().addBefore("decoder", "via-decoder", new ChannelInboundHandlerAdapter() {
                @Override public void channelRead(ChannelHandlerContext context, Object message) {
                    context.fireChannelRead(message);
                }
            }));
        foton.network.FotonViaChannel via = foton.network.FotonViaBridge.open();
        if (via == null) throw new AssertionError("open() returned null");
        try {
            io.netty.channel.Channel channel = channelOf(via);
            io.netty.channel.ChannelPipeline pipeline = channel.pipeline();

            // PipelineUtil.getPreviousContext, as ViaVersion 5.12.1 writes it.
            String previous = null;
            ChannelHandlerContext before = null;
            for (String entry : pipeline.toMap().keySet()) {
                if (entry.equals("via-decoder")) { before = pipeline.context(previous); break; }
                previous = entry;
            }
            if (previous == null) throw new AssertionError(
                "Via's decoder is the first handler, so re-injecting a packet throws: " + pipeline.names());

            // The packet fired from there must come out of the pipeline as serverbound output.
            io.netty.buffer.ByteBuf packet = io.netty.buffer.Unpooled.buffer();
            packet.writeBytes(new byte[] {1, 2, 3});
            before.fireChannelRead(packet);
            byte[][] output = via.poll();
            if (output.length != 1 || output[0][0] != 0 || output[0].length != 4)
                throw new AssertionError("re-injected packet did not come out serverbound: " + output.length);
        } finally {
            via.close();
        }
        System.out.println("ViaPipelineCheck OK");
    }

    private static io.netty.channel.Channel channelOf(foton.network.FotonViaChannel via) throws Exception {
        java.util.UUID id = java.util.UUID.randomUUID();
        foton.network.FotonViaBridge.bind(id.toString(), via);
        return foton.network.FotonViaBridge.channelOf(id);
    }
}
