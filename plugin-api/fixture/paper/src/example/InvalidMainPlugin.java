package example;

import org.bukkit.plugin.java.JavaPlugin;

public final class InvalidMainPlugin extends JavaPlugin {
    public InvalidMainPlugin() {
        System.setProperty("foton.fixture.paper.invalid.main", "constructed");
    }
}
