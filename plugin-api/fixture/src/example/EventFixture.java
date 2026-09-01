package example;

import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.event.EventHandler;
import org.bukkit.event.EventPriority;
import org.bukkit.event.Listener;
import org.bukkit.event.block.BlockBreakEvent;
import org.bukkit.event.player.AsyncPlayerChatEvent;
import org.bukkit.event.player.PlayerJoinEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Exercises the parts of the event path that are easy to get wrong. */
public final class EventFixture extends JavaPlugin implements Listener, org.bukkit.command.CommandExecutor {
    /** What the last command run against this plugin saw. */
    public static String commandLabel;
    public static String[] commandArgs;
    public static String commandSender;
    /** What the plugin read out of its own configuration. */
    public static String greeting;
    public static int addedLater;
    public static String nested;

    /** Counts what the scheduler actually ran, read back by the check. */
    public static int immediate = 0;
    public static int delayed = 0;
    public static int repeating = 0;

    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);

        // Exactly what a plugin does: lay the jar's config.yml down if the
        // operator has none, then read through it.
        saveDefaultConfig();
        greeting = getConfig().getString("greeting");
        addedLater = getConfig().getInt("added-later");
        nested = getConfig().getString("nested.value");

        // Scheduling must not run anything. That is the whole promise: the
        // body waits for a tick, on the thread where the world is safe.
        getServer().getScheduler().runTask(this, () -> immediate++);
        getServer().getScheduler().runTaskLater(this, () -> delayed++, 2);
        getServer().getScheduler().runTaskTimer(this, () -> repeating++, 0, 2);
    }

    /** The plugin is its own executor, which is how most are written.
     *
     * Answering false for no arguments makes the host print the usage line
     * from plugin.yml, which is the only argument checking a great many
     * plugins have.
     */
    @Override
    public boolean onCommand(CommandSender sender, Command command, String label, String[] args) {
        commandLabel = label;
        commandArgs = args;
        commandSender = sender.getName();
        if (args.length == 0) {
            return false;
        }
        sender.sendMessage("fixture ran with " + String.join(" ", args));
        return true;
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
