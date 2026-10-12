package foton.network;

/** The play-state packet listener a player handle carries; see {@link Connection}. */
public final class ServerGamePacketListenerImpl {
    public final Connection connection;

    ServerGamePacketListenerImpl(Connection connection) { this.connection = connection; }
}
