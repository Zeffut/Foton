package example;

import java.sql.DriverManager;
import org.bukkit.plugin.java.JavaPlugin;

/** Connects through a provider that exists only on the host class path. */
public final class JdbcPlugin extends JavaPlugin {
    @Override
    public void onEnable() {
        try (var ignored = DriverManager.getConnection("jdbc:foton-host:fixture")) {
            // Acquiring the connection is the behavior under test.
        } catch (java.sql.SQLException error) {
            throw new IllegalStateException(error);
        }
    }
}
