package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.ServicePriority;
import org.bukkit.plugin.java.JavaPlugin;
import fixture.LifecycleProbe;

/** Claims every plugin-owned resource kind and then fails from onLoad. */
public final class FailingLoadPlugin extends JavaPlugin implements Listener {
    @Override
    public void onLoad() {
        LifecycleProbe.called("FailingLoad.load");
        LifecycleProbe.loaderOwned("FailingLoad.loader", getClass().getClassLoader());
        LifecycleProbe.loaderOwned("FailingLoad.eventLoader",
            FailingEvent.class.getClassLoader());
        LifecycleProbe.flag("FailingLoad.visible",
            getServer().getPluginManager().getPlugin(getName()) == this);
        getServer().getPluginManager().registerEvents(this, this);
        Runnable syncTask = () -> {};
        LifecycleProbe.loaderOwned("FailingLoad.lambdaLoader",
            syncTask.getClass().getClassLoader());
        getServer().getScheduler().runTaskTimer(this, syncTask, 100, 100);
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
        LifecycleProbe.called("FailingLoad.disable");
    }
}
