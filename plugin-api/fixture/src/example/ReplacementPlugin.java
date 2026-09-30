package example;

import fixture.LifecycleProbe;
import org.bukkit.plugin.java.JavaPlugin;

/** Healthy generation loaded under the failed fixture's plugin name. */
public final class ReplacementPlugin extends JavaPlugin {
    @Override
    public void onLoad() {
        LifecycleProbe.called("Replacement.load");
        LifecycleProbe.loaderOwned("Replacement.loader", getClass().getClassLoader());
    }

    @Override
    public void onEnable() {
        LifecycleProbe.called("Replacement.enable");
        getServer().getScheduler().runTask(
            this, () -> LifecycleProbe.called("Replacement.taskRun"));
    }

    @Override
    public void onDisable() {
        LifecycleProbe.called("Replacement.disable");
    }
}
