package example;

import org.bukkit.plugin.java.JavaPlugin;
import fixture.LifecycleProbe;

/** Must never enter either lifecycle phase after its required dependency fails to load. */
public final class LoadDependentPlugin extends JavaPlugin {
    @Override public void onLoad() { LifecycleProbe.called("LoadDependent.load"); }
    @Override public void onEnable() { LifecycleProbe.called("LoadDependent.enable"); }
}
