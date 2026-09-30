package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Must not enable after its required provider is disabled from the event. */
public final class EventDisabledDependentPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        System.setProperty("foton.fixture.eventDisableDependentEnableCalls", "1");
    }
}
