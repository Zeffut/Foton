package example;

import org.bukkit.plugin.java.JavaPlugin;

public final class FallbackPlugin extends JavaPlugin {
    public FallbackPlugin() {
        System.setProperty("foton.fixture.paper.fallback.constructor", "true");
    }

    @Override
    public void onEnable() {
        System.setProperty("foton.fixture.paper.fallback.enable", "true");
    }
}
