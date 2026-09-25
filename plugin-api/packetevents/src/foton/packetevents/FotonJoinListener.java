package foton.packetevents;

import com.github.retrooper.packetevents.PacketEvents;
import com.github.retrooper.packetevents.event.UserLoginEvent;
import com.github.retrooper.packetevents.protocol.player.User;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.event.player.PlayerQuitEvent;

/** Gives a user its Bukkit player the moment one exists.
 *
 * Until the join, {@code event.getPlayer()} on a packet event is null, as it
 * is on Paper; from the lowest-priority join handler on, every other plugin's
 * join handler already sees packets carrying the player.
 */
final class FotonJoinListener implements Listener {
    private final FotonTap tap;

    FotonJoinListener(FotonTap tap) { this.tap = tap; }

    @EventHandler(priority = EventPriority.LOWEST)
    public void bind(PlayerJoinEvent event) {
        FotonChannel channel = tap.channel(event.getPlayer().getUniqueId());
        if (channel != null) channel.player = event.getPlayer();
    }

    /** Last, so every other quit handler still finds the player's channel. */
    @EventHandler(priority = EventPriority.MONITOR)
    public void release(PlayerQuitEvent event) {
        tap.quit(event.getPlayer().getUniqueId());
    }

    @EventHandler
    public void login(PlayerJoinEvent event) {
        User user = PacketEvents.getAPI().getPlayerManager().getUser(event.getPlayer());
        if (user != null) {
            PacketEvents.getAPI().getEventManager().callEvent(new UserLoginEvent(user, event.getPlayer()));
        }
    }
}
