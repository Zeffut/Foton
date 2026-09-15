package example;

import org.bukkit.plugin.java.JavaPlugin;

public final class HealthyPaperPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        System.setProperty("foton.fixture.paper.healthy", "enabled");
    }
}
