package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Must never enter either lifecycle phase after its required dependency fails to load. */
public final class LoadDependentPlugin extends JavaPlugin {
    public static int loadCalls;
    public static int enableCalls;

    @Override public void onLoad() { loadCalls++; }
    @Override public void onEnable() { enableCalls++; }
}
