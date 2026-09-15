package example;

import org.bukkit.plugin.java.JavaPlugin;
import fixture.LifecycleProbe;

/** Loads normally, but must not enable after its required dependency fails to enable. */
public final class EnableDependentPlugin extends JavaPlugin {
    @Override public void onLoad() {
        LifecycleProbe.called("EnableDependent.load");
        LifecycleProbe.loaderOwned("EnableDependent.loader", getClass().getClassLoader());
    }

    @Override public void onEnable() { LifecycleProbe.called("EnableDependent.enable"); }
}
