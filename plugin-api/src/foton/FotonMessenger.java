package foton;

import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Objects;
import java.util.Set;
import java.util.UUID;
import java.util.logging.Level;
import org.bukkit.entity.Player;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.messaging.ChannelNameTooLongException;
import org.bukkit.plugin.messaging.ChannelNotRegisteredException;
import org.bukkit.plugin.messaging.MessageTooLargeException;
import org.bukkit.plugin.messaging.Messenger;
import org.bukkit.plugin.messaging.PluginMessageListener;
import org.bukkit.plugin.messaging.PluginMessageListenerRegistration;
import org.bukkit.plugin.messaging.ReservedChannelException;

/** Bukkit custom channels backed by Foton's custom-payload packets. */
final class FotonMessenger implements Messenger {
    private final Map<Plugin, Set<String>> outgoing = new HashMap<>();
    private final Map<String, Set<PluginMessageListenerRegistration>> incomingByChannel =
        new HashMap<>();
    private final Map<Plugin, Set<PluginMessageListenerRegistration>> incomingByPlugin =
        new HashMap<>();
    private final Map<UUID, Set<String>> listening = new HashMap<>();

    @Override public boolean isReservedChannel(String channel) {
        String corrected = channel(channel);
        return corrected.equals("minecraft:register")
            || corrected.equals("minecraft:unregister");
    }

    @Override public synchronized void registerOutgoingPluginChannel(
            Plugin plugin, String channel) {
        Objects.requireNonNull(plugin, "plugin");
        String corrected = channel(channel);
        rejectReserved(corrected);
        outgoing.computeIfAbsent(plugin, ignored -> new HashSet<>()).add(corrected);
    }

    @Override public synchronized void unregisterOutgoingPluginChannel(
            Plugin plugin, String channel) {
        Objects.requireNonNull(plugin, "plugin");
        Set<String> channels = outgoing.get(plugin);
        if (channels == null) {
            return;
        }
        channels.remove(channel(channel));
        if (channels.isEmpty()) {
            outgoing.remove(plugin);
        }
    }

    @Override public synchronized void unregisterOutgoingPluginChannel(Plugin plugin) {
        outgoing.remove(Objects.requireNonNull(plugin, "plugin"));
    }

    @Override public synchronized PluginMessageListenerRegistration
            registerIncomingPluginChannel(
                Plugin plugin, String channel, PluginMessageListener listener) {
        Objects.requireNonNull(plugin, "plugin");
        Objects.requireNonNull(listener, "listener");
        String corrected = channel(channel);
        rejectReserved(corrected);
        PluginMessageListenerRegistration registration =
            new PluginMessageListenerRegistration(this, plugin, corrected, listener);
        Set<PluginMessageListenerRegistration> onChannel =
            incomingByChannel.computeIfAbsent(corrected, ignored -> new HashSet<>());
        if (!onChannel.add(registration)) {
            throw new IllegalArgumentException("This registration already exists");
        }
        incomingByPlugin.computeIfAbsent(plugin, ignored -> new HashSet<>()).add(registration);
        return registration;
    }

    @Override public synchronized void unregisterIncomingPluginChannel(
            Plugin plugin, String channel, PluginMessageListener listener) {
        remove(new PluginMessageListenerRegistration(
            this,
            Objects.requireNonNull(plugin, "plugin"),
            channel(channel),
            Objects.requireNonNull(listener, "listener")));
    }

    @Override public synchronized void unregisterIncomingPluginChannel(
            Plugin plugin, String channel) {
        Objects.requireNonNull(plugin, "plugin");
        String corrected = channel(channel);
        for (PluginMessageListenerRegistration registration
                : Set.copyOf(incomingByPlugin.getOrDefault(plugin, Set.of()))) {
            if (registration.getChannel().equals(corrected)) {
                remove(registration);
            }
        }
    }

    @Override public synchronized void unregisterIncomingPluginChannel(Plugin plugin) {
        Objects.requireNonNull(plugin, "plugin");
        for (PluginMessageListenerRegistration registration
                : Set.copyOf(incomingByPlugin.getOrDefault(plugin, Set.of()))) {
            remove(registration);
        }
    }

    @Override public synchronized Set<String> getOutgoingChannels() {
        Set<String> channels = new HashSet<>();
        for (Set<String> registered : outgoing.values()) {
            channels.addAll(registered);
        }
        return Set.copyOf(channels);
    }

    @Override public synchronized Set<String> getOutgoingChannels(Plugin plugin) {
        Objects.requireNonNull(plugin, "plugin");
        return Set.copyOf(outgoing.getOrDefault(plugin, Set.of()));
    }

    @Override public synchronized Set<String> getIncomingChannels() {
        return Set.copyOf(incomingByChannel.keySet());
    }

    @Override public synchronized Set<String> getIncomingChannels(Plugin plugin) {
        Set<String> channels = new HashSet<>();
        for (PluginMessageListenerRegistration registration
                : getIncomingChannelRegistrations(plugin)) {
            channels.add(registration.getChannel());
        }
        return Set.copyOf(channels);
    }

    @Override public synchronized Set<PluginMessageListenerRegistration>
            getIncomingChannelRegistrations(Plugin plugin) {
        Objects.requireNonNull(plugin, "plugin");
        return Set.copyOf(incomingByPlugin.getOrDefault(plugin, Set.of()));
    }

    @Override public synchronized Set<PluginMessageListenerRegistration>
            getIncomingChannelRegistrations(String channel) {
        return Set.copyOf(incomingByChannel.getOrDefault(channel(channel), Set.of()));
    }

    @Override public synchronized Set<PluginMessageListenerRegistration>
            getIncomingChannelRegistrations(Plugin plugin, String channel) {
        String corrected = channel(channel);
        Set<PluginMessageListenerRegistration> found = new HashSet<>();
        for (PluginMessageListenerRegistration registration
                : getIncomingChannelRegistrations(plugin)) {
            if (registration.getChannel().equals(corrected)) {
                found.add(registration);
            }
        }
        return Set.copyOf(found);
    }

    @Override public synchronized boolean isRegistrationValid(
            PluginMessageListenerRegistration registration) {
        Objects.requireNonNull(registration, "registration");
        return registration.getPlugin().isEnabled()
            && incomingByPlugin.getOrDefault(registration.getPlugin(), Set.of())
                .contains(registration);
    }

    @Override public synchronized boolean isOutgoingChannelRegistered(
            Plugin plugin, String channel) {
        Objects.requireNonNull(plugin, "plugin");
        return outgoing.getOrDefault(plugin, Set.of()).contains(channel(channel));
    }

    @Override public synchronized boolean isIncomingChannelRegistered(
            Plugin plugin, String channel) {
        return !getIncomingChannelRegistrations(plugin, channel).isEmpty();
    }

    @Override public void dispatchIncomingMessage(Player player, String channel, byte[] message) {
        Objects.requireNonNull(player, "player");
        Objects.requireNonNull(message, "message");
        String corrected = channel(channel);
        for (PluginMessageListenerRegistration registration
                : getIncomingChannelRegistrations(corrected)) {
            if (!registration.isValid()) {
                continue;
            }
            try {
                registration.getListener().onPluginMessageReceived(corrected, player, message);
            } catch (Throwable error) {
                registration.getPlugin().getLogger().log(Level.WARNING,
                    "Plugin " + registration.getPlugin().getName()
                        + " threw while handling plugin message",
                    error);
            }
        }
    }

    /** Entry called from Rust for a serverbound custom payload. */
    public static void dispatchFromNetwork(String playerId, String channel, byte[] message) {
        FotonMessenger messenger = running();
        UUID uuid = Native.parse(playerId);
        Player player = uuid == null ? null : org.bukkit.Bukkit.getPlayer(uuid);
        if (messenger == null || player == null) {
            return;
        }
        if (channel.equals("minecraft:register") || channel.equals("minecraft:unregister")) {
            messenger.updateListening(player, channel, message);
            return;
        }
        messenger.dispatchIncomingMessage(player, channel, message);
    }

    static Set<String> listening(UUID player) {
        FotonMessenger messenger = running();
        if (messenger == null) {
            return Set.of();
        }
        synchronized (messenger) {
            return Set.copyOf(messenger.listening.getOrDefault(player, Set.of()));
        }
    }

    static void forgetPlayer(String playerId) {
        FotonMessenger messenger = running();
        UUID player = Native.parse(playerId);
        if (messenger == null || player == null) {
            return;
        }
        synchronized (messenger) {
            messenger.listening.remove(player);
        }
    }

    static void send(Player player, Plugin plugin, String channel, byte[] message) {
        FotonMessenger messenger = running();
        if (messenger == null) {
            throw new IllegalStateException("No plugin messenger is running");
        }
        Objects.requireNonNull(player, "player");
        Objects.requireNonNull(plugin, "plugin");
        Objects.requireNonNull(message, "message");
        String corrected = channel(channel);
        if (!plugin.isEnabled()) {
            throw new IllegalArgumentException("Plugin must be enabled to send messages");
        }
        if (!messenger.isOutgoingChannelRegistered(plugin, corrected)) {
            throw new ChannelNotRegisteredException(corrected);
        }
        if (message.length > MAX_MESSAGE_SIZE) {
            throw new MessageTooLargeException(message);
        }
        Native.sendPluginMessage(
            player.getUniqueId().toString(), corrected, message);
    }

    private synchronized void updateListening(Player player, String operation, byte[] payload) {
        UUID playerId = player.getUniqueId();
        Set<String> channels = listening.computeIfAbsent(playerId, ignored -> new HashSet<>());
        for (String candidate : new String(payload, StandardCharsets.UTF_8).split("\\0")) {
            try {
                String corrected = channel(candidate);
                if (operation.equals("minecraft:register") && channels.add(corrected)) {
                    EventBridge.dispatch(new org.bukkit.event.player.PlayerRegisterChannelEvent(
                        player, corrected));
                } else if (operation.equals("minecraft:unregister")
                        && channels.remove(corrected)) {
                    EventBridge.dispatch(new org.bukkit.event.player.PlayerUnregisterChannelEvent(
                        player, corrected));
                }
            } catch (IllegalArgumentException ignored) {
                // Client input: an invalid advertised channel is ignored.
            }
        }
        if (channels.isEmpty()) {
            listening.remove(playerId);
        }
    }

    private synchronized void remove(PluginMessageListenerRegistration registration) {
        Set<PluginMessageListenerRegistration> channel =
            incomingByChannel.get(registration.getChannel());
        if (channel != null) {
            channel.remove(registration);
            if (channel.isEmpty()) {
                incomingByChannel.remove(registration.getChannel());
            }
        }
        Set<PluginMessageListenerRegistration> plugin =
            incomingByPlugin.get(registration.getPlugin());
        if (plugin != null) {
            plugin.remove(registration);
            if (plugin.isEmpty()) {
                incomingByPlugin.remove(registration.getPlugin());
            }
        }
    }

    private static FotonMessenger running() {
        org.bukkit.Server server = org.bukkit.Bukkit.getServer();
        return server != null && server.getMessenger() instanceof FotonMessenger messenger
            ? messenger : null;
    }

    private static String channel(String channel) {
        Objects.requireNonNull(channel, "channel");
        if (channel.equals("BungeeCord")) {
            return "bungeecord:main";
        }
        if (channel.length() > MAX_CHANNEL_SIZE) {
            throw new ChannelNameTooLongException(channel.length());
        }
        if (!channel.matches("[a-z0-9._-]+:[a-z0-9/._-]+")) {
            throw new IllegalArgumentException("Invalid plugin channel '" + channel + "'");
        }
        return channel;
    }

    private static void rejectReserved(String channel) {
        if (channel.equals("minecraft:register") || channel.equals("minecraft:unregister")) {
            throw new ReservedChannelException(channel);
        }
    }
}
