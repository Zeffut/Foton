package example;

import org.bukkit.plugin.java.JavaPlugin;

/** Exercises the supported self-disable path from inside onEnable. */
public final class SelfDisablingPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        System.setProperty("foton.fixture.selfDisableEnableCalls", "1");
        getServer().getPluginManager().disablePlugin(this);
    }

    @Override
    public void onDisable() {
        System.setProperty("foton.fixture.selfDisableDisableCalls",
            Integer.toString(Integer.getInteger(
                "foton.fixture.selfDisableDisableCalls", 0) + 1));
    }
}
