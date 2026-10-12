package foton.network;

/** What {@code CraftPlayer#getHandle()} returns: the part of Minecraft's {@code ServerPlayer} that
 * plugins reach for to get at a player's network channel, and nothing else.
 *
 * <p>The handle is built on demand and holds the channel as it is when asked, so a player whose
 * connection has no Via pipeline yields a handle whose {@code connection.connection.channel} is null.
 */
public final class ServerPlayerHandle {
    public final ServerGamePacketListenerImpl connection;

    public ServerPlayerHandle(java.util.UUID player) {
        this.connection = new ServerGamePacketListenerImpl(new Connection(FotonViaBridge.channelOf(player)));
    }
}
