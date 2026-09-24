package foton.packetevents;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.util.mappings.GlobalRegistryHolder;
import com.github.retrooper.packetevents.manager.server.ServerVersion;
import com.github.retrooper.packetevents.netty.NettyManager;
import com.github.retrooper.packetevents.netty.buffer.ByteBufAllocationOperator;
import com.github.retrooper.packetevents.netty.buffer.ByteBufOperator;
import com.github.retrooper.packetevents.netty.channel.ChannelOperator;
import com.github.retrooper.packetevents.protocol.ProtocolVersion;
import com.github.retrooper.packetevents.protocol.player.ClientVersion;
import com.github.retrooper.packetevents.protocol.player.User;
import io.github.retrooper.packetevents.impl.netty.buffer.ByteBufAllocationOperatorImpl;
import io.github.retrooper.packetevents.impl.netty.buffer.ByteBufOperatorImpl;
import io.github.retrooper.packetevents.impl.netty.manager.player.PlayerManagerAbstract;
import io.github.retrooper.packetevents.impl.netty.manager.protocol.ProtocolManagerAbstract;
import io.github.retrooper.packetevents.impl.netty.manager.server.ServerManagerAbstract;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;

/** The managers PacketEvents asks a platform for.
 *
 * Buffers are real Netty buffers, from PacketEvents' own Netty module: its
 * wrappers read and write through them, and Foton has no reason to supply a
 * second implementation. Channels are Foton's, see {@link FotonChannel}.
 */
final class FotonManagers {
    private FotonManagers() {}

    static final class Netty implements NettyManager {
        private final ChannelOperator channels = new FotonChannelOperator();
        private final ByteBufOperator buffers = new ByteBufOperatorImpl();
        private final ByteBufAllocationOperator allocation = new ByteBufAllocationOperatorImpl();

        @Override public ChannelOperator getChannelOperator() { return channels; }
        @Override public ByteBufOperator getByteBufOperator() { return buffers; }
        @Override public ByteBufAllocationOperator getByteBufAllocationOperator() { return allocation; }
    }

    static final class Server extends ServerManagerAbstract {
        private volatile ServerVersion version;

        /** The release Foton reports, which is the protocol it speaks.
         *
         * Matched by release name first, like the Spigot platform, and by
         * protocol number only if the name is one this PacketEvents has never
         * heard of -- the newest release sharing the protocol is the one whose
         * packet layout is right.
         */
        @Override public ServerVersion getVersion() {
            ServerVersion known = version;
            if (known != null) return known;
            String release = Bukkit.getMinecraftVersion();
            for (ServerVersion candidate : ServerVersion.reversedValues()) {
                if (candidate.getReleaseName().equals(release)) return version = candidate;
            }
            int protocol = Bukkit.getUnsafe().getProtocolVersion();
            for (ServerVersion candidate : ServerVersion.reversedValues()) {
                if (candidate.getProtocolVersion() == protocol) return version = candidate;
            }
            PacketEvents.getAPI().getLogManager().warn(
                "Foton speaks Minecraft " + release + " (protocol " + protocol
                    + "), which this PacketEvents does not know; assuming " + ServerVersion.getLatest().getReleaseName());
            return version = ServerVersion.getLatest();
        }

        @Override public Object getRegistryCacheKey(User user, ClientVersion clientVersion) {
            return GlobalRegistryHolder.getGlobalRegistryCacheKey(user, clientVersion);
        }
    }

    static final class Protocol extends ProtocolManagerAbstract {
        @Override public ProtocolVersion getPlatformVersion() {
            return ProtocolVersion.UNKNOWN;
        }
    }

    static final class Players extends PlayerManagerAbstract {
        @Override public int getPing(Object player) {
            return ((Player) player).getPing();
        }

        @Override public Object getChannel(Object player) {
            return PacketEvents.getAPI().getProtocolManager().getChannel(((Player) player).getUniqueId());
        }
    }
}
