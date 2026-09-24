package foton.packetevents;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.event.PacketReceiveEvent;
import com.github.retrooper.packetevents.event.PacketSendEvent;
import com.github.retrooper.packetevents.event.UserConnectEvent;
import com.github.retrooper.packetevents.manager.protocol.ProtocolManager;
import com.github.retrooper.packetevents.netty.buffer.ByteBufHelper;
import com.github.retrooper.packetevents.netty.buffer.UnpooledByteBufAllocationHelper;
import com.github.retrooper.packetevents.protocol.ConnectionState;
import com.github.retrooper.packetevents.protocol.player.ClientVersion;
import com.github.retrooper.packetevents.protocol.player.User;
import com.github.retrooper.packetevents.protocol.player.UserProfile;
import com.github.retrooper.packetevents.util.PacketEventsImplHelper;
import foton.Native;
import foton.PacketBridge;
import java.net.InetSocketAddress;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

/** Where Foton's packet tap meets PacketEvents' event pipeline.
 *
 * Each packet is put in a buffer as id and payload -- the shape PacketEvents'
 * decoder and encoder see on a Netty server, between framing and the game --
 * and handed to the same {@link PacketEventsImplHelper} those handlers call.
 * What the helper leaves in the buffer is the answer: nothing readable means
 * a listener cancelled it, a different buffer means a wrapper re-encoded it.
 */
final class FotonTap implements PacketBridge.Handler {
    private final Map<Long, FotonChannel> channels = new ConcurrentHashMap<>();

    FotonChannel channel(UUID player) {
        Object channel = PacketEvents.getAPI().getProtocolManager().getChannel(player);
        return channel instanceof FotonChannel foton ? foton : null;
    }

    @Override
    public void opened(long connection, UUID profile, String name, String address) {
        FotonChannel channel = new FotonChannel(connection, profile, name, parse(address));
        ProtocolManager protocol = PacketEvents.getAPI().getProtocolManager();
        ClientVersion version = ClientVersion.getById(PacketEvents.getAPI().getServerManager().getVersion().getProtocolVersion());
        User user = new User(channel, ConnectionState.CONFIGURATION, version, new UserProfile(profile, name));
        channels.put(connection, channel);
        protocol.setUser(channel, user);
        protocol.setChannel(profile, channel);

        UserConnectEvent connect = new UserConnectEvent(user);
        PacketEvents.getAPI().getEventManager().callEvent(connect);
        if (connect.isCancelled()) {
            // A connection refused this early has no player to kick yet; the
            // closest Foton can do is make every packet of it vanish.
            channel.open = false;
        }
    }

    @Override
    public void playing(long connection, UUID player, int entityId) {
        FotonChannel channel = channels.get(connection);
        if (channel == null) return;
        User user = PacketEvents.getAPI().getProtocolManager().getUser(channel);
        if (user == null) return;
        user.setEntityId(entityId);
        user.setConnectionState(ConnectionState.PLAY);
    }

    @Override
    public void closed(long connection) {
        FotonChannel channel = channels.remove(connection);
        if (channel == null) return;
        channel.open = false;
        channel.afterSend.clear();
        PacketEventsImplHelper.handleDisconnection(channel, channel.uuid);
    }

    @Override
    public int inbound(long connection, int phase, int packetId, byte[] payload) {
        FotonChannel channel = channels.get(connection);
        if (channel == null) return PacketBridge.PASS;
        if (!channel.open) return PacketBridge.CANCEL;
        User user = PacketEvents.getAPI().getProtocolManager().getUser(channel);
        if (user == null) return PacketBridge.PASS;
        // Foton knows the phase for certain; the user's own idea of it is
        // only as good as the packets it has seen.
        ConnectionState state = state(phase);
        if (user.getDecoderState() != state) user.setDecoderState(state);

        Object buffer = frame(packetId, payload);
        try {
            PacketReceiveEvent event = PacketEventsImplHelper.handleServerBoundPacket(channel, user, channel.player, buffer, true);
            return answer(channel, packetId, buffer, event != null && event.getLastUsedWrapper() != null, false);
        } catch (Throwable error) {
            return failed(user, packetId, error);
        } finally {
            ByteBufHelper.release(buffer);
        }
    }

    @Override
    public int outbound(long connection, int phase, int packetId, byte[] payload) {
        FotonChannel channel = channels.get(connection);
        if (channel == null) return PacketBridge.PASS;
        User user = PacketEvents.getAPI().getProtocolManager().getUser(channel);
        if (user == null) return PacketBridge.PASS;
        ConnectionState state = state(phase);
        if (user.getEncoderState() != state) user.setEncoderState(state);

        Object buffer = frame(packetId, payload);
        try {
            PacketSendEvent event = PacketEventsImplHelper.handleClientBoundPacket(channel, user, channel.player, buffer, true);
            int answer = answer(channel, packetId, buffer, event != null && event.getLastUsedWrapper() != null, true);
            if (answer != PacketBridge.CANCEL && event != null && event.hasTasksAfterSend()) {
                channel.afterSend.add(new ArrayList<>(event.getTasksAfterSend()));
                answer |= PacketBridge.AFTER_SEND;
            }
            return answer;
        } catch (Throwable error) {
            return failed(user, packetId, error);
        } finally {
            ByteBufHelper.release(buffer);
        }
    }

    @Override
    public void sent(long connection) {
        FotonChannel channel = channels.get(connection);
        if (channel == null) return;
        List<Runnable> tasks = channel.afterSend.poll();
        if (tasks == null) return;
        for (Runnable task : tasks) {
            try {
                task.run();
            } catch (Throwable error) {
                PacketEvents.getAPI().getLogManager().warn("A task after sending a packet to " + channel.name + " failed", error);
            }
        }
    }

    /** The answer the helper left in the buffer, in the tap's terms. */
    private static int answer(FotonChannel channel, int packetId, Object buffer, boolean reEncoded, boolean outbound) {
        if (!ByteBufHelper.isReadable(buffer)) return PacketBridge.CANCEL;
        if (!reEncoded) return PacketBridge.PASS;
        int id = ByteBufHelper.readVarInt(buffer);
        byte[] rewritten = new byte[ByteBufHelper.readableBytes(buffer)];
        ByteBufHelper.readBytes(buffer, rewritten);
        if (id == packetId) {
            PacketBridge.rewrite(rewritten);
            return PacketBridge.REWRITE;
        }
        // A listener turned the packet into a different one. The tap keeps
        // ids, so the original is dropped and the new one goes its way
        // silently -- its listeners have just seen it.
        String player = channel.uuid.toString();
        if (outbound) Native.packetSend(player, id, rewritten, true);
        else Native.packetReceive(player, id, rewritten, true);
        return PacketBridge.CANCEL;
    }

    /** A listener threw. The packet goes through, as it would on Netty with kick-on-exception off. */
    private static int failed(User user, int packetId, Throwable error) {
        if (PacketEvents.getAPI().getSettings().isFullStackTraceEnabled()) {
            PacketEvents.getAPI().getLogManager().warn(
                "An error occurred while processing packet " + packetId + " for " + user.getName(), error);
        } else {
            PacketEvents.getAPI().getLogManager().warn(String.valueOf(error.getMessage()));
        }
        return PacketBridge.PASS;
    }

    private static Object frame(int packetId, byte[] payload) {
        Object buffer = UnpooledByteBufAllocationHelper.buffer(payload.length + 5);
        ByteBufHelper.writeVarInt(buffer, packetId);
        ByteBufHelper.writeBytes(buffer, payload);
        return buffer;
    }

    private static ConnectionState state(int phase) {
        return phase == PacketBridge.CONFIGURATION ? ConnectionState.CONFIGURATION : ConnectionState.PLAY;
    }

    private static InetSocketAddress parse(String address) {
        int colon = address.lastIndexOf(':');
        if (colon < 0) return InetSocketAddress.createUnresolved(address, 0);
        String host = address.substring(0, colon);
        if (host.startsWith("[") && host.endsWith("]")) host = host.substring(1, host.length() - 1);
        try {
            return new InetSocketAddress(java.net.InetAddress.getByName(host), Integer.parseInt(address.substring(colon + 1)));
        } catch (Exception malformed) {
            return InetSocketAddress.createUnresolved(host, 0);
        }
    }
}
