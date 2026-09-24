package foton.packetevents;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.PacketEventsAPI;
import com.github.retrooper.packetevents.injector.ChannelInjector;
import com.github.retrooper.packetevents.manager.player.PlayerManager;
import com.github.retrooper.packetevents.manager.protocol.ProtocolManager;
import com.github.retrooper.packetevents.manager.server.ServerManager;
import com.github.retrooper.packetevents.netty.NettyManager;
import com.github.retrooper.packetevents.protocol.player.User;
import foton.PacketBridge;
import java.util.Locale;
import org.bukkit.Bukkit;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;

/** PacketEvents' API, standing on Foton's packet tap instead of a Netty pipeline.
 *
 * This is the Foton counterpart of PacketEvents' own Spigot builder. The
 * difference is the injector: Spigot splices handlers into every channel's
 * pipeline, Foton offers every packet to one handler, so injecting is turning
 * the tap on and nothing else, and it happens in init() rather than load().
 */
final class FotonPacketEvents {
    private FotonPacketEvents() {}

    static PacketEventsAPI<Plugin> build(Plugin plugin) {
        FotonTap tap = new FotonTap();
        return new PacketEventsAPI<Plugin>() {
            private final ProtocolManager protocolManager = new FotonManagers.Protocol();
            private final ServerManager serverManager = new FotonManagers.Server();
            private final PlayerManager playerManager = new FotonManagers.Players();
            private final NettyManager nettyManager = new FotonManagers.Netty();
            private final ChannelInjector injector = new Injector(tap);
            private boolean loaded;
            private boolean initialized;
            private boolean terminated;

            @Override
            public void load() {
                if (loaded) return;
                String id = plugin.getName().toLowerCase(Locale.ROOT);
                PacketEvents.IDENTIFIER = "pe-" + id;
                PacketEvents.ENCODER_NAME = "pe-encoder-" + id;
                PacketEvents.DECODER_NAME = "pe-decoder-" + id;
                PacketEvents.CONNECTION_HANDLER_NAME = "pe-connection-handler-" + id;
                PacketEvents.SERVER_CHANNEL_HANDLER_NAME = "pe-connection-initializer-" + id;
                PacketEvents.TIMEOUT_HANDLER_NAME = "pe-timeout-handler-" + id;
                super.load();
                loaded = true;
                getLogManager().info("Loaded packetevents v" + getVersion() + " on Foton's packet tap");
            }

            @Override public boolean isLoaded() { return loaded; }

            @Override
            public void init() {
                load();
                if (initialized) return;
                // Here rather than in load(): a plugin's onLoad runs before
                // Foton's natives are bound to the server, and the tap has no
                // server to attach to until then. Nothing is lost -- Foton
                // opens its port only after every plugin is enabled.
                injector.inject();
                Bukkit.getPluginManager().registerEvents(new FotonJoinListener(tap), plugin);
                // A reload finds players already online; bind them as a join would.
                for (Player player : Bukkit.getOnlinePlayers()) {
                    FotonChannel channel = tap.channel(player.getUniqueId());
                    if (channel != null) channel.player = player;
                }
                initialized = true;
            }

            @Override public boolean isInitialized() { return initialized; }

            @Override
            public void terminate() {
                if (!initialized) return;
                super.terminate();
                initialized = false;
                terminated = true;
            }

            @Override public boolean isTerminated() { return terminated; }
            @Override public Plugin getPlugin() { return plugin; }
            @Override public ServerManager getServerManager() { return serverManager; }
            @Override public ProtocolManager getProtocolManager() { return protocolManager; }
            @Override public PlayerManager getPlayerManager() { return playerManager; }
            @Override public NettyManager getNettyManager() { return nettyManager; }
            @Override public ChannelInjector getInjector() { return injector; }
        };
    }

    /** Injecting, on Foton, is turning the packet tap on. */
    private static final class Injector implements ChannelInjector {
        private final FotonTap tap;
        private boolean injected;

        Injector(FotonTap tap) { this.tap = tap; }

        @Override
        public void inject() {
            if (injected) return;
            if (!PacketBridge.install(tap)) {
                throw new IllegalStateException("Foton refused to install the packet tap");
            }
            injected = true;
        }

        @Override
        public void uninject() {
            if (!injected) return;
            PacketBridge.uninstall(tap);
            injected = false;
        }

        @Override public void updateUser(Object channel, User user) {}

        @Override
        public void setPlayer(Object channel, Object player) {
            if (channel instanceof FotonChannel foton) foton.player = (Player) player;
        }

        @Override
        public boolean isPlayerSet(Object channel) {
            return channel instanceof FotonChannel foton && foton.player != null;
        }

        @Override public boolean isProxy() { return false; }
    }
}
