import java.io.File;
import java.nio.file.Files;
import java.util.List;
import org.bukkit.configuration.ConfigurationSection;
import org.bukkit.configuration.file.YamlConfiguration;

/** The configuration API, on the behavior plugins actually rely on. */
final class Config {
    private Config() {}

    static void check() throws Exception {
        YamlConfiguration config = new YamlConfiguration();
        config.loadFromString(String.join("\n",
            "messages:",
            "  prefix: '[Example] '",
            "  joined: welcome",
            "limits:",
            "  players: 20",
            "  ratio: 1.5",
            "worlds.world_nether: true",
            "banned:",
            "- alice",
            "- bob",
            ""));

        Checks.same(config.getString("messages.prefix"), "[Example] ", "a nested string");
        Checks.same(config.getInt("limits.players"), 20, "a nested int");

        // getInt on a stored double coerces rather than answering 0: plugins
        // write `1.5` in a config and read it with getInt often enough that
        // Bukkit does the same.
        Checks.same(config.getInt("limits.ratio"), 1, "getInt coerces a double");
        Checks.same(config.getDouble("limits.ratio"), 1.5, "getDouble keeps the fraction");

        // A dotted key in the file is a path, which is Bukkit's own behavior.
        Checks.expect(config.getBoolean("worlds.world_nether"),
            "a dotted key should be reachable as a path");

        // The defaults that differ by type, and that plugins do not check.
        Checks.same(config.getString("nothing.here"), null, "a missing string is null");
        Checks.same(config.getInt("nothing.here"), 0, "a missing int is zero");
        Checks.expect(!config.getBoolean("nothing.here"), "a missing boolean is false");
        Checks.expect(config.getList("nothing.here") == null, "a missing list is null");
        // This one is the trap: an absent string list is EMPTY, not null, and
        // plugins iterate it without checking.
        Checks.expect(config.getStringList("nothing.here").isEmpty(),
            "a missing string list must be an empty list, not null");
        Checks.same(config.getStringList("banned"), List.of("alice", "bob"), "a string list");

        Checks.expect(config.contains("messages.prefix"), "contains finds a set path");
        Checks.expect(!config.contains("messages.absent"), "contains rejects a missing path");

        ConfigurationSection messages = config.getConfigurationSection("messages");
        Checks.expect(messages != null, "a section should come back as a section");
        Checks.same(messages.getString("joined"), "welcome", "a getter on a section");
        Checks.same(messages.getCurrentPath(), "messages", "a section knows where it is");
        Checks.same(config.getConfigurationSection("messages.prefix"), null,
            "a scalar is not a section");

        Checks.same(config.getKeys(false), new java.util.LinkedHashSet<>(
            List.of("messages", "limits", "worlds", "banned")), "the shallow keys");
        Checks.expect(config.getKeys(true).contains("messages.prefix"),
            "the deep keys include nested paths");

        // Setting null removes.
        config.set("messages.joined", null);
        Checks.expect(!config.contains("messages.joined"), "setting null should remove");

        // Setting builds the levels on the way.
        config.set("a.b.c", "deep");
        Checks.same(config.getString("a.b.c"), "deep", "set builds intermediate sections");

        defaults();
        file();
    }

    /** A default answers for a path the file does not have. */
    private static void defaults() {
        YamlConfiguration config = new YamlConfiguration();
        config.addDefault("limits.players", 10);
        Checks.same(config.getInt("limits.players"), 10, "a default answers a missing path");
        config.set("limits.players", 30);
        Checks.same(config.getInt("limits.players"), 30, "a set value beats its default");
    }

    /** Saving and loading a real file, which is what saveConfig does. */
    private static void file() throws Exception {
        File directory = Files.createTempDirectory("foton-config").toFile();
        File target = new File(directory, "config.yml");

        YamlConfiguration written = new YamlConfiguration();
        written.set("messages.prefix", "[Example] ");
        written.set("limits.players", 20);
        written.set("banned", List.of("alice", "bob"));
        written.save(target);

        Checks.expect(target.isFile(), "save should have written the file");
        YamlConfiguration read = YamlConfiguration.loadConfiguration(target);
        Checks.same(read.getString("messages.prefix"), "[Example] ", "a saved string reads back");
        Checks.same(read.getInt("limits.players"), 20, "a saved int reads back");
        Checks.same(read.getStringList("banned"), List.of("alice", "bob"),
            "a saved list reads back");

        // A file that is not there is an empty configuration, not a crash: a
        // plugin calling this in onEnable would otherwise take the server down.
        YamlConfiguration absent =
            YamlConfiguration.loadConfiguration(new File(directory, "no-such-file.yml"));
        Checks.expect(absent.getKeys(false).isEmpty(), "a missing file reads as empty");

        target.delete();
        directory.delete();
    }
}
