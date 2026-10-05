import java.io.File;
import java.io.StringReader;
import java.nio.file.Files;
import java.util.List;
import java.util.Map;
import org.bukkit.configuration.ConfigurationSection;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.plugin.InvalidDescriptionException;
import org.bukkit.plugin.PluginDescriptionFile;

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

        options();
        defaults();
        file();
        pluginDescriptors();
    }

    /** The options chain, which only works if every setter narrows. */
    private static void options() {
        YamlConfiguration config = new YamlConfiguration();

        // This exact line appears in thirty-three of the fifty-nine plugins
        // surveyed. It compiles only because `copyDefaults` returns the type
        // that declares `header`, which is the whole reason the options
        // classes exist as a hierarchy rather than one class.
        config.options().copyDefaults(true).header("written by the check").indent(4);

        Checks.expect(config.options().copyDefaults(), "copyDefaults stuck");
        Checks.same(config.options().header(), "written by the check", "header stuck");
        Checks.same(config.options().indent(), 4, "indent stuck");
        Checks.expect(config.options().configuration() == config,
            "the options know their configuration");
    }

    /** A default answers for a path the file does not have. */
    private static void defaults() {
        YamlConfiguration config = new YamlConfiguration();
        config.addDefault("limits.players", 10);
        Checks.same(config.getInt("limits.players"), 10, "a default answers a missing path");
        Checks.expect(config.contains("limits.players"),
            "contains includes a configured default");
        Checks.expect(!config.contains("limits.players", true),
            "contains can explicitly ignore defaults");
        Checks.expect(!config.isSet("limits.players"),
            "isSet ignores a value that exists only in defaults");
        config.set("limits.players", 30);
        Checks.same(config.getInt("limits.players"), 30, "a set value beats its default");
        Checks.expect(config.contains("limits.players", true) && config.isSet("limits.players"),
            "an explicitly set value is present even when defaults are ignored");
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

    private static void pluginDescriptors() throws Exception {
        PluginDescriptionFile descriptor = descriptor("""
            name: Provider
            version: 1
            main: example.Provider
            api-version: '26.2'
            depend: [Required]
            softdepend: [Optional]
            loadbefore: [Consumer]
            provides: [Vault]
            libraries: [com.example:fixture-lib:1.0]
            bootstrapper: example.ProviderBootstrap
            loader: example.ProviderLoader
            dependencies:
              bootstrap:
                Registry:
                  load: BEFORE
                  required: false
                  join-classpath: true
              server:
                Economy:
                  load: AFTER
                  required: true
                  join-classpath: false
                Defaults: {}
            """);

        Checks.same(descriptor.getDepend(), List.of("Required"), "legacy depend");
        Checks.same(descriptor.getSoftDepend(), List.of("Optional"), "legacy softdepend");
        Checks.same(descriptor.getLoadBefore(), List.of("Consumer"), "legacy loadbefore");
        Checks.same(descriptor.getProvides(), List.of("Vault"), "provides");
        Checks.same(descriptor.getLibraries(), List.of("com.example:fixture-lib:1.0"),
            "libraries");
        Checks.same(descriptor.getBootstrapper(), "example.ProviderBootstrap", "bootstrapper");
        Checks.same(descriptor.getLoader(), "example.ProviderLoader", "loader");

        PluginDescriptionFile.Dependency bootstrap =
            descriptor.getBootstrapDependencies().get("Registry");
        Checks.same(bootstrap.load(), PluginDescriptionFile.Load.BEFORE,
            "Paper bootstrap dependency direction");
        Checks.expect(!bootstrap.required(), "Paper bootstrap dependency required flag");
        Checks.expect(bootstrap.joinClasspath(),
            "Paper bootstrap dependency join-classpath flag");

        PluginDescriptionFile.Dependency server =
            descriptor.getServerDependencies().get("Economy");
        Checks.same(server.load(), PluginDescriptionFile.Load.AFTER,
            "Paper server dependency direction");
        Checks.expect(server.required(), "Paper server dependency required flag");
        Checks.expect(!server.joinClasspath(),
            "Paper server dependency join-classpath flag");
        Checks.same(descriptor.getServerDependencies().get("Defaults"),
            new PluginDescriptionFile.Dependency(
                PluginDescriptionFile.Load.OMIT, true, true),
            "Paper dependency defaults");

        expectImmutable(descriptor.getProvides(), "provides list");
        expectImmutable(descriptor.getLibraries(), "libraries list");
        expectImmutable(descriptor.getServerDependencies(), "server dependency map");

        expectInvalid("""
            name: '   '
            version: 1
            main: example.Blank
            """, "name");
        expectInvalid("""
            name: DuplicateAlias
            version: 1
            main: example.DuplicateAlias
            provides: [Vault, vault]
            """, "provides");
        expectInvalid("""
            name: BlankDependency
            version: 1
            main: example.BlankDependency
            depend: ['  ']
            """, "depend");
        expectInvalid("""
            name: Future
            version: 1
            main: example.Future
            api-version: '26.3'
            """, "api-version");
        expectInvalid("""
            name: BadDirection
            version: 1
            main: example.BadDirection
            dependencies:
              server:
                Economy:
                  load: SIDEWAYS
            """, "dependencies.server.Economy.load");
        expectInvalid("""
            name: BadSection
            version: 1
            main: example.BadSection
            dependencies:
              server: [Economy]
            """, "dependencies.server");
        expectInvalid("""
            name: BadBody
            version: 1
            main: example.BadBody
            dependencies:
              bootstrap:
                Registry: BEFORE
            """, "dependencies.bootstrap.Registry");
    }

    private static PluginDescriptionFile descriptor(String yaml)
            throws InvalidDescriptionException {
        return new PluginDescriptionFile(new StringReader(yaml));
    }

    @SuppressWarnings({"rawtypes", "unchecked"})
    private static void expectImmutable(Object value, String what) {
        try {
            if (value instanceof List list) {
                list.add("mutated");
            } else if (value instanceof Map map) {
                map.put("mutated", "mutated");
            }
            throw new AssertionError(what + " is mutable");
        } catch (UnsupportedOperationException expected) {
            // Expected: descriptor collections are safe to share with plugins.
        }
    }

    private static void expectInvalid(String yaml, String field) throws Exception {
        try {
            descriptor(yaml);
            throw new AssertionError(field + " descriptor was accepted");
        } catch (InvalidDescriptionException expected) {
            Checks.expect(expected.getMessage() != null
                    && expected.getMessage().contains(field),
                field + " validation did not name its field: " + expected.getMessage());
        }
    }
}
