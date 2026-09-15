package example;

import org.bukkit.plugin.java.JavaPlugin;
import fixture.LifecycleProbe;

/** Proves one plugin's lifecycle failure does not stop an unrelated plugin. */
public final class HealthyPlugin extends JavaPlugin {
    @Override public void onLoad() {
        LifecycleProbe.called("Healthy.load");
        LifecycleProbe.loaderOwned("Healthy.loader", getClass().getClassLoader());
    }

    @Override public void onEnable() { LifecycleProbe.called("Healthy.enable"); }
}
