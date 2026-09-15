package fixture.dependencies;

import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.java.JavaPlugin;

public class DependencyPlugin extends JavaPlugin {
    @Override
    public void onLoad() {
        append("foton.fixture.dependencies.load");
        if (getName().equals("Consumer")) {
            Plugin provider = getServer().getPluginManager().getPlugin("vAuLt");
            System.setProperty("foton.fixture.dependencies.alias",
                provider == null ? "missing" : provider.getName());
        }
        if (getName().equals("PaperConsumer")) {
            System.setProperty("foton.fixture.dependencies.joined",
                visible("fixture.dependencies.BootstrapApi") ? "true" : "false");
        }
        if (getName().equals("NoJoinConsumer")) {
            System.setProperty("foton.fixture.dependencies.isolated",
                visible("fixture.dependencies.NoJoinApi") ? "false" : "true");
        }
    }

    @Override
    public void onEnable() {
        append("foton.fixture.dependencies.enable");
    }

    private void append(String property) {
        String existing = System.getProperty(property, "");
        System.setProperty(property,
            existing.isEmpty() ? getName() : existing + "," + getName());
    }

    private boolean visible(String className) {
        try {
            Class.forName(className, false, getClass().getClassLoader());
            return true;
        } catch (ClassNotFoundException expected) {
            return false;
        }
    }
}
