package foton.fixture.channels;

import org.bukkit.command.PluginCommand;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerRegisterChannelEvent;
import org.bukkit.event.player.PlayerUnregisterChannelEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** A Paper-compiled plugin exercising the live custom-payload path. */
public final class PluginChannelProbe extends JavaPlugin implements Listener {
    private static final String OUTGOING = "fixture:voice";
    private static final String INCOMING = "fixture:client";
    private static final byte[] RESPONSE = {(byte) 0x55, (byte) 0x66};

    @Override public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getMessenger().registerOutgoingPluginChannel(this, OUTGOING);
        getServer().getMessenger().registerIncomingPluginChannel(this, INCOMING,
            (channel, player, payload) -> {
                if (payload.length == 2 && payload[0] == 0x33 && payload[1] == 0x44) {
                    System.out.println("[channel-probe] INCOMING_RECEIVED");
                }
            });
        PluginCommand command = getCommand("channelprobe");
        if (command == null) {
            throw new IllegalStateException("fixture command was not registered");
        }
        command.setExecutor((sender, ignored, label, args) -> {
            if (!(sender instanceof Player player)) return false;
            player.sendPluginMessage(this, OUTGOING, RESPONSE);
            System.out.println("[channel-probe] COMMAND_SENT");
            return true;
        });
        System.out.println("[channel-probe] ENABLED");
    }

    @EventHandler public void onRegister(PlayerRegisterChannelEvent event) {
        if (!OUTGOING.equals(event.getChannel())) return;
        if (!event.getPlayer().getListeningPluginChannels().contains(OUTGOING)) {
            throw new IllegalStateException("REGISTER callback observed a missing channel");
        }
        event.getPlayer().sendPluginMessage(this, OUTGOING, RESPONSE);
        System.out.println("[channel-probe] REGISTERED_AND_SENT");
    }

    @EventHandler public void onUnregister(PlayerUnregisterChannelEvent event) {
        if (!OUTGOING.equals(event.getChannel())) return;
        if (event.getPlayer().getListeningPluginChannels().contains(OUTGOING)) {
            throw new IllegalStateException("UNREGISTER callback observed a present channel");
        }
        event.getPlayer().sendPluginMessage(this, OUTGOING, RESPONSE);
        System.out.println("[channel-probe] UNREGISTERED_AND_SUPPRESSED");
    }
}
