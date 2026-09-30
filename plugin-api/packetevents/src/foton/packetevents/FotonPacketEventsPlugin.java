package foton.packetevents;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.protocol.packettype.PacketType;
import com.github.retrooper.packetevents.protocol.player.ClientVersion;
import com.github.retrooper.packetevents.util.TimeStampMode;
import foton.Native;
import org.bukkit.plugin.java.JavaPlugin;

/** The {@code packetevents} plugin, as Foton ships it.
 *
 * The upstream PacketEvents plugin cannot run here: it finds Minecraft's
 * Netty pipeline by reflection into CraftBukkit, and Foton has neither. This
 * one carries PacketEvents' own, unmodified API and implements only what the
 * upstream Spigot module implements -- where packets come from and where they
 * go -- on Foton's packet tap. Plugins that depend on {@code packetevents}
 * find it under that name and get the same API, the same wrappers and the same
 * settings the upstream plugin applies.
 */
public final class FotonPacketEventsPlugin extends JavaPlugin {
    @Override
    public void onLoad() {
        PacketEvents.setAPI(FotonPacketEvents.build(this));
        PacketEvents.getAPI().load();
    }

    @Override
    public void onEnable() {
        // What the upstream plugin sets, except the update check: this build
        // is tied to Foton's release, and pointing an operator at a newer
        // upstream jar would point them at one that cannot run here.
        PacketEvents.getAPI().getSettings()
            .debug(false)
            .checkForUpdates(false)
            .timeStampMode(TimeStampMode.MILLIS)
            .reEncodeByDefault(true);
        saveDefaultConfig();
        spareChunkPackets(!getConfig().getBoolean("show-chunk-packets", false));
        PacketEvents.getAPI().init();
    }

    /** Keeps chunk and light data away from listeners unless asked for.
     *
     * Foton encodes a broadcast once for every recipient, so by the time the
     * tap sees a chunk it is framed and compressed. Showing it to PacketEvents
     * costs an inflate and a copy per packet per player, for the packets that
     * are most of a server's bandwidth -- and almost no plugin reads them.
     * One that does (an anti-xray, say) turns them back on in config.yml.
     */
    private void spareChunkPackets(boolean spare) {
        ClientVersion version = ClientVersion.getById(
            PacketEvents.getAPI().getServerManager().getVersion().getProtocolVersion());
        for (PacketType.Play.Server type : new PacketType.Play.Server[] {
                PacketType.Play.Server.CHUNK_DATA,
                PacketType.Play.Server.UPDATE_LIGHT,
                PacketType.Play.Server.CHUNK_BIOMES}) {
            int id = type.getId(version);
            if (id >= 0) Native.packetTapSkipOutbound(id, spare);
        }
    }

    @Override
    public void onDisable() {
        PacketEvents.getAPI().terminate();
    }
}
