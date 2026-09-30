package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Is disabled synchronously by the observer during PluginEnableEvent. */
public final class EventDisabledPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        System.setProperty("foton.fixture.eventDisableEnableCalls", "1");
    }

    @Override
    public void onDisable() {
        System.setProperty("foton.fixture.eventDisableDisableCalls",
            Integer.toString(Integer.getInteger(
                "foton.fixture.eventDisableDisableCalls", 0) + 1));
    }
}
