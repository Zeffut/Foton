package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Proves one plugin's lifecycle failure does not stop an unrelated plugin. */
public final class HealthyPlugin extends JavaPlugin {
    public static int loadCalls;
    public static int enableCalls;

    @Override public void onLoad() { loadCalls++; }
    @Override public void onEnable() { enableCalls++; }
}
