/** Binary and behavioral checks for Paper's channel initialization hook. */
final class NetworkHooks {
    private NetworkHooks() {}

    static void check() {
        net.kyori.adventure.key.Key first =
            net.kyori.adventure.key.Key.key("foton:first");
        net.kyori.adventure.key.Key second =
            net.kyori.adventure.key.Key.key("foton:second");
        java.util.List<String> calls = new java.util.ArrayList<>();

        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(
            first, channel -> calls.add("first"));
        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(
            second, channel -> calls.add("second"));
        Checks.expect(
            io.papermc.paper.network.ChannelInitializeListenerHolder.hasListener(first),
            "Paper channel listener was not registered");

        io.netty.channel.embedded.EmbeddedChannel channel =
            new io.netty.channel.embedded.EmbeddedChannel();
        try {
            io.papermc.paper.network.ChannelInitializeListenerHolder.callListeners(channel);
        } finally {
            channel.finishAndReleaseAll();
        }
        Checks.same(calls, java.util.List.of("first", "second"),
            "Paper channel listeners did not retain registration order");

        Checks.expect(
            io.papermc.paper.network.ChannelInitializeListenerHolder.removeListener(first)
                != null,
            "Paper channel listener removal lost the registered listener");
        io.papermc.paper.network.ChannelInitializeListenerHolder.removeListener(second);
        Checks.expect(
            io.papermc.paper.network.ChannelInitializeListenerHolder.getListeners().isEmpty(),
            "Paper channel listener registry leaked listeners after removal");
        try {
            io.papermc.paper.network.ChannelInitializeListenerHolder.getListeners().put(
                first, ignored -> {});
            throw new AssertionError("Paper channel listener view was mutable");
        } catch (UnsupportedOperationException expected) {
            // Paper exposes a live, immutable view.
        }

        Checks.expect(foton.network.FotonViaBridge.open() == null,
            "Foton opened a translation channel without a registered injector");
        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(first, candidate -> {});
        try (foton.network.FotonViaChannel bridge = foton.network.FotonViaBridge.open()) {
            byte[][] inbound = bridge.serverbound(new byte[] {0, 1, 2});
            Checks.expect(inbound.length == 1 && inbound[0][0] == 0,
                "Foton Via channel lost an inbound packet");
            byte[][] outbound = bridge.clientbound(new byte[] {3, 4});
            Checks.expect(outbound.length == 1 && outbound[0][0] == 1,
                "Foton Via channel lost an outbound packet");
            Checks.expect(bridge.poll().length == 0,
                "Foton Via channel retained already-drained packets");
        }
        io.papermc.paper.network.ChannelInitializeListenerHolder.removeListener(first);

        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(first, candidate ->
            candidate.pipeline().addBefore("decoder", "hostile-release", new io.netty.channel.ChannelInboundHandlerAdapter() {
                @Override public void channelRead(io.netty.channel.ChannelHandlerContext context, Object message) {
                    io.netty.util.ReferenceCountUtil.release(message);
                    throw new IllegalStateException("released by hostile handler");
                }
            }));
        try (foton.network.FotonViaChannel bridge = foton.network.FotonViaBridge.open()) {
            try {
                bridge.serverbound(new byte[] {0});
                throw new AssertionError("Via bridge swallowed a handler failure");
            } catch (IllegalStateException expected) {
                Checks.expect(expected.getMessage().contains("released by hostile handler"),
                    "Via bridge masked a handler failure after ownership transfer");
            }
        }
        io.papermc.paper.network.ChannelInitializeListenerHolder.removeListener(first);

        io.papermc.paper.network.ChannelInitializeListenerHolder.addListener(first, candidate -> {
            candidate.pipeline().fireChannelRead(candidate.alloc().buffer().writeByte(1));
            throw new IllegalStateException("constructor failure");
        });
        try {
            foton.network.FotonViaBridge.open();
            throw new AssertionError("Via bridge swallowed a constructor failure");
        } catch (IllegalStateException expected) {
            Checks.expect(expected.getMessage().contains("constructor failure"),
                "Via bridge masked a constructor cleanup failure");
        }
        io.papermc.paper.network.ChannelInitializeListenerHolder.removeListener(first);
    }
}
