package foton.packetevents;

import java.net.InetSocketAddress;
import java.util.List;
import java.util.Queue;
import java.util.UUID;
import java.util.concurrent.ConcurrentLinkedQueue;
import org.bukkit.entity.Player;

/** What PacketEvents calls a channel: one client connection, as Foton names it.
 *
 * On a Netty server this is the Netty channel and PacketEvents reaches into
 * its pipeline. Here there is no pipeline; the connection id Foton handed out
 * is the identity, and everything a pipeline would have done is a call across
 * the packet tap.
 */
final class FotonChannel {
    final long connection;
    final UUID uuid;
    final String name;
    final InetSocketAddress address;
    volatile Player player;
    volatile boolean open = true;
    /** Work waiting for a written packet, one entry per packet, in write order. */
    final Queue<List<Runnable>> afterSend = new ConcurrentLinkedQueue<>();

    FotonChannel(long connection, UUID uuid, String name, InetSocketAddress address) {
        this.connection = connection;
        this.uuid = uuid;
        this.name = name;
        this.address = address;
    }

    @Override
    public String toString() {
        return "FotonChannel[" + connection + " " + name + "]";
    }
}
