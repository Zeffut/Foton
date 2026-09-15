package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.ServicePriority;
import org.bukkit.plugin.java.JavaPlugin;

/** Claims every plugin-owned resource kind and then fails from onLoad. */
public final class FailingLoadPlugin extends JavaPlugin implements Listener {
    public static FailingLoadPlugin instance;
    public static int disableCalls;
    public static boolean visibleDuringLoad;

    @Override
    public void onLoad() {
        instance = this;
        visibleDuringLoad = getServer().getPluginManager().getPlugin(getName()) == this;
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getScheduler().runTaskTimer(this, () -> {}, 100, 100);
        getServer().getScheduler().runTaskTimerAsynchronously(
            this, () -> {}, 100, 100);
        try {
            getServer().getServicesManager().register(
                Runnable.class, () -> {}, this, ServicePriority.Normal);
        } catch (UnsatisfiedLinkError unavailableInJavaFixture) {
            // Registration happens before the server event asks the absent JNI test host.
        }
        getServer().getMessenger().registerIncomingPluginChannel(
            this, "failing:load", (channel, player, message) -> {});
        getServer().getMessenger().registerOutgoingPluginChannel(this, "failing:load");
        throw new IllegalStateException("failing from onLoad");
    }

    @EventHandler
    public void onFailing(FailingEvent event) {}

    @Override
    public void onDisable() {
        disableCalls++;
    }
}
