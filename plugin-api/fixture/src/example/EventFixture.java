package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Exercises the parts of the event path that are easy to get wrong. */
public final class EventFixture extends JavaPlugin implements Listener {
    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
    }

    /** Rewrites the announcement, which proves a change travels back. */
    @EventHandler
    public void onJoin(PlayerJoinEvent event) {
        event.setJoinMessage("rewritten by the fixture");
    }

    /** Cancels, which proves a veto travels back. */
    @EventHandler
    public void onChat(AsyncPlayerChatEvent event) {
        if (event.getMessage().contains("hush")) {
            event.setCancelled(true);
        }
    }

    /** Runs first and cancels, so the next handler must not see it. */
    @EventHandler(priority = EventPriority.LOWEST)
    public void onBreakFirst(BlockBreakEvent event) {
        event.setCancelled(true);
    }

    /** Would undo the cancel, but must never run: it did not opt in. */
    @EventHandler(priority = EventPriority.HIGH)
    public void onBreakLater(BlockBreakEvent event) {
        event.setCancelled(false);
    }
}
