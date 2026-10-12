package foton.network;

/** What a plugin that reflects on CraftBukkit finds behind a player's connection handle.
 *
 * ViaVersion walks {@code CraftPlayer#getHandle()} to a field whose simple type name is
 * {@code ServerGamePacketListenerImpl}, from there to one named {@code Connection}, and then to the
 * field of type {@code io.netty.channel.Channel}. The simple names are the contract, so they are
 * Mojang's. There is no Minecraft connection object behind it: {@link #channel} is the
 * per-connection Via pipeline Foton opened for the player's TCP connection, or null when no plugin
 * registered a Paper channel listener.
 */
public final class Connection {
    public final io.netty.channel.Channel channel;

    Connection(io.netty.channel.Channel channel) { this.channel = channel; }
}
