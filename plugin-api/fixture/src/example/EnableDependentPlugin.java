package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Loads normally, but must not enable after its required dependency fails to enable. */
public final class EnableDependentPlugin extends JavaPlugin {
    public static int loadCalls;
    public static int enableCalls;

    @Override public void onLoad() { loadCalls++; }
    @Override public void onEnable() { enableCalls++; }
}
