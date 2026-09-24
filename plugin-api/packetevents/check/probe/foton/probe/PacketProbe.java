package foton.probe;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.event.PacketListenerAbstract;
import com.github.retrooper.packetevents.event.PacketListenerCommon;
import com.github.retrooper.packetevents.event.PacketListenerPriority;
import com.github.retrooper.packetevents.event.PacketReceiveEvent;
import com.github.retrooper.packetevents.event.PacketSendEvent;
import com.github.retrooper.packetevents.protocol.packettype.PacketType;
import com.github.retrooper.packetevents.wrapper.play.client.WrapperPlayClientPlayerFlying;
import com.github.retrooper.packetevents.wrapper.play.server.WrapperPlayServerPing;
import java.nio.file.Files;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import org.bukkit.entity.Player;
import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** A plugin that uses PacketEvents the way real ones do, and writes down what it saw.
 *
 * dev/plugin-compat-test.sh reads the report: a probe is the only way to tell
 * "the packet pipeline works" from "nothing crashed".
 */
public final class PacketProbe extends JavaPlugin implements Listener {
    private final Map<String, AtomicInteger> seen = new ConcurrentHashMap<>();
    private final Map<String, String> facts = new ConcurrentHashMap<>();
    private PacketListenerCommon listener;

    private void count(String what) { seen.computeIfAbsent(what, k -> new AtomicInteger()).incrementAndGet(); }

    @Override
    public void onEnable() {
        listener = PacketEvents.getAPI().getEventManager().registerListener(new PacketListenerAbstract(PacketListenerPriority.MONITOR) {
            @Override
            public void onPacketReceive(PacketReceiveEvent event) {
                count("in " + event.getPacketType());
                if (WrapperPlayClientPlayerFlying.isFlying(event.getPacketType())) {
                    WrapperPlayClientPlayerFlying flying = new WrapperPlayClientPlayerFlying(event);
                    if (flying.hasPositionChanged()) {
                        facts.put("last position", flying.getLocation().getX() + " " + flying.getLocation().getY() + " " + flying.getLocation().getZ());
                    }
                    if (event.getPlayer() instanceof Player player) {
                        facts.put("flying carries the player", player.getName());
                    }
                }
                if (event.getPacketType() == PacketType.Configuration.Client.PLUGIN_MESSAGE
                        || event.getPacketType() == PacketType.Configuration.Client.CLIENT_SETTINGS) {
                    facts.put("configuration user", String.valueOf(event.getUser().getName()));
                }
            }

            @Override
            public void onPacketSend(PacketSendEvent event) {
                count("out " + event.getPacketType());
                if (event.getPacketType() == PacketType.Play.Server.PING) {
                    WrapperPlayServerPing ping = new WrapperPlayServerPing(event);
                    if (ping.getId() == 424242) facts.put("tapped ping seen", "yes");
                    if (ping.getId() == 434343) facts.put("silent ping seen", "yes (wrong)");
                    event.getTasksAfterSend().add(() -> facts.put("after-send ran for ping", String.valueOf(ping.getId())));
                }
                if (event.getPacketType() == PacketType.Play.Server.JOIN_GAME) {
                    facts.put("entity id at join", String.valueOf(event.getUser().getEntityId()));
                }
            }
        });
        getServer().getPluginManager().registerEvents(this, this);
    }

    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        Player player = event.getPlayer();
        facts.put("bukkit entity id", String.valueOf(player.getEntityId()));
        var user = PacketEvents.getAPI().getPlayerManager().getUser(player);
        facts.put("user found for player", String.valueOf(user != null));
        if (user == null) return;
        facts.put("user entity id", String.valueOf(user.getEntityId()));
        facts.put("client version", user.getClientVersion().getReleaseName());
        facts.put("server version", PacketEvents.getAPI().getServerManager().getVersion().getReleaseName());
        user.sendPacket(new WrapperPlayServerPing(424242));
        user.sendPacketSilently(new WrapperPlayServerPing(434343));
    }

    @Override
    public void onDisable() {
        if (listener != null) PacketEvents.getAPI().getEventManager().unregisterListener(listener);
        StringBuilder report = new StringBuilder();
        new TreeMap<>(facts).forEach((k, v) -> report.append("fact ").append(k).append(" = ").append(v).append('\n'));
        new TreeMap<>(seen).forEach((k, v) -> report.append("count ").append(k).append(" = ").append(v.get()).append('\n'));
        try {
            getDataFolder().mkdirs();
            Files.writeString(getDataFolder().toPath().resolve("report.txt"), report.toString());
        } catch (Exception error) {
            getLogger().severe("could not write the probe report: " + error);
        }
    }
}
