package example;

import org.bukkit.event.EventHandler;
import org.bukkit.event.Listener;
import org.bukkit.plugin.ServicePriority;
import org.bukkit.plugin.java.JavaPlugin;
import fixture.LifecycleProbe;

/** Claims every plugin-owned resource kind and then fails from onEnable. */
public final class FailingEnablePlugin extends JavaPlugin implements Listener {
    @Override
    public void onLoad() {
        LifecycleProbe.called("FailingEnable.load");
        LifecycleProbe.loaderOwned("FailingEnable.loader", getClass().getClassLoader());
        LifecycleProbe.flag("scheduler.gameTickBlocked",
            foton.LifecycleDiagnostics.gameTickApisBlockOnLifecycleMonitor(this));
    }

    @Override
    public void onEnable() {
        LifecycleProbe.called("FailingEnable.enable");
        getServer().getPluginManager().registerEvents(this, this);
        getServer().getScheduler().runTaskTimer(this, () -> {}, 100, 100);
        try {
            getServer().getServicesManager().register(
                Runnable.class, () -> {}, this, ServicePriority.Normal);
        } catch (UnsatisfiedLinkError unavailableInJavaFixture) {
            // Registration happens before the server event asks the absent JNI test host.
        }
        getServer().getMessenger().registerIncomingPluginChannel(
            this, "failing:enable", (channel, player, message) -> {});
        getServer().getMessenger().registerOutgoingPluginChannel(this, "failing:enable");
        getServer().getScheduler().runTaskAsynchronously(this, () -> {
            LifecycleProbe.asyncStarted();
            try {
                LifecycleProbe.awaitRelease();
                submitFollowUp(false);
                submitFollowUp(true);
            } finally {
                LifecycleProbe.asyncFinished();
            }
        });
        org.bukkit.scheduler.BukkitTask tickLocal =
            getServer().getScheduler().runTaskTimer(
                this, () -> LifecycleProbe.called("FailingEnable.keptRun"), 100, 1);
        LifecycleProbe.flag("FailingEnable.removedFromQueue",
            foton.LifecycleDiagnostics.removeQueuedTask(tickLocal));
        getServer().getScheduler().runTaskTimer(this, () -> {
            LifecycleProbe.called("FailingEnable.syncRun");
            LifecycleProbe.syncStarted();
            try {
                LifecycleProbe.awaitSyncRelease();
                foton.LifecycleDiagnostics.republishQueuedTask(tickLocal);
            } finally {
                LifecycleProbe.syncFinished();
            }
        }, 0, 1);
        Thread tick = new Thread(foton.FotonScheduler::tick, "fixture-sync-race");
        tick.start();
        LifecycleProbe.awaitAsyncStarted();
        LifecycleProbe.awaitSyncStarted();
        throw new IllegalStateException("failing from onEnable");
    }

    private void submitFollowUp(boolean async) {
        boolean rejected = false;
        try {
            if (async) {
                getServer().getScheduler().runTaskLaterAsynchronously(
                    this, LifecycleProbe::followUpRan, 100);
            } else {
                getServer().getScheduler().runTask(this, LifecycleProbe::followUpRan);
            }
        } catch (IllegalStateException expected) {
            rejected = true;
        }
        LifecycleProbe.followUpAttempt(rejected);
    }

    @EventHandler
    public void onFailing(FailingEvent event) {}

    @Override
    public void onDisable() {
        LifecycleProbe.called("FailingEnable.disable");
        LifecycleProbe.flag("FailingEnable.disabledSawFalse", !isEnabled());
        foton.PluginHost.cleanup(this);
    }
}
