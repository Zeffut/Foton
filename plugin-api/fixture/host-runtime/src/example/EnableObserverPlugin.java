package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.event.server.PluginEnableEvent;
import org.bukkit.plugin.java.JavaPlugin;

/** Observes whether a self-disabled plugin receives a false enable event. */
public final class EnableObserverPlugin extends JavaPlugin implements Listener {
    @Override
    public void onEnable() {
        getServer().getPluginManager().registerEvents(this, this);
    }

    @EventHandler
    public void onPluginEnable(PluginEnableEvent event) {
        if (event.getPlugin().getName().equals("SelfDisabling")) {
            System.setProperty("foton.fixture.selfDisableEnableEvents",
                Integer.toString(Integer.getInteger(
                    "foton.fixture.selfDisableEnableEvents", 0) + 1));
        }
        if (event.getPlugin().getName().equals("EventDisabled")) {
            System.setProperty("foton.fixture.eventDisableEnableEvents",
                Integer.toString(Integer.getInteger(
                    "foton.fixture.eventDisableEnableEvents", 0) + 1));
            getServer().getPluginManager().disablePlugin(event.getPlugin());
            System.setProperty("foton.fixture.eventDisableEnabledAfterCleanup",
                Boolean.toString(event.getPlugin().isEnabled()));
        }
    }
}
