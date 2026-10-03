package foton.fixture.sqlite;

import java.sql.Connection;
import java.sql.Driver;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.Statement;
import org.bukkit.plugin.java.JavaPlugin;

/** Exercises DriverManager from a real plugin class loader. */
public final class SqlitePlugin extends JavaPlugin {
    @Override public void onEnable() {
        try {
            Class<?> visibleDriver = Class.forName("org.sqlite.JDBC", false, getClass().getClassLoader());
            Driver registeredDriver = DriverManager.getDriver("jdbc:sqlite::memory:");
            if (visibleDriver != registeredDriver.getClass()) {
                throw new IllegalStateException("plugin and DriverManager see different SQLite classes");
            }
            try (Connection connection = DriverManager.getConnection("jdbc:sqlite::memory:");
                 Statement statement = connection.createStatement()) {
                statement.executeUpdate("CREATE TABLE compatibility_probe (value INTEGER NOT NULL)");
                statement.executeUpdate("INSERT INTO compatibility_probe VALUES (42)");
                try (ResultSet result = statement.executeQuery("SELECT value FROM compatibility_probe")) {
                    if (!result.next() || result.getInt(1) != 42 || result.next()) {
                        throw new IllegalStateException("SQLite roundtrip returned unexpected rows");
                    }
                }
            }
            System.out.println("[sqlite-plugin] DriverManager SQL roundtrip passed");
        } catch (Exception error) {
            throw new IllegalStateException("SQLite JDBC is unusable from the plugin", error);
        }
    }
}
