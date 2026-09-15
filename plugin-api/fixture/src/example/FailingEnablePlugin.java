package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.ServicePriority;
import org.bukkit.plugin.java.JavaPlugin;

/** Claims every plugin-owned resource kind and then fails from onEnable. */
public final class FailingEnablePlugin extends JavaPlugin implements Listener {
    public static FailingEnablePlugin instance;
    public static boolean disabledSawFalse;
    public static int disableCalls;

    @Override
    public void onEnable() {
        instance = this;
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
            this, "failing:enable", (channel, player, message) -> {});
        getServer().getMessenger().registerOutgoingPluginChannel(this, "failing:enable");
        throw new IllegalStateException("failing from onEnable");
    }

    @EventHandler
    public void onFailing(FailingEvent event) {}

    @Override
    public void onDisable() {
        disableCalls++;
        disabledSawFalse = !isEnabled();
    }
}
