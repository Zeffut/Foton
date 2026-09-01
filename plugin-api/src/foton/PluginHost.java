package foton;

import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.jar.JarFile;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.PluginDescriptionFile;
import org.bukkit.plugin.java.JavaPlugin;

/** Finds plugin jars, loads them, and enables them.
 *
 * Class loading and reflection live here rather than behind JNI because this
 * is what the JVM is good at, and every line of it written in Rust would be
 * three lines that do the same thing less clearly.
 */
public final class PluginHost {
    private static final List<Plugin> loaded = new ArrayList<>();

    private PluginHost() {}

    /** Loads and enables every plugin in a directory. Returns how many worked. */
    public static int loadAll(String directory) {
        org.bukkit.Bukkit.setServer(new FotonServer());
        File dir = new File(directory);
        File[] jars = dir.listFiles((d, name) -> name.endsWith(".jar"));
        if (jars == null) {
            System.out.println("[host] no plugin directory at " + directory);
            return 0;
        }
        int enabled = 0;
        for (File jar : jars) {
            try {
                if (load(jar)) {
                    enabled++;
                }
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
            }
        }
        return enabled;
    }

    private static boolean load(File jar) throws Exception {
        Descriptor descriptor;
        try (JarFile archive = new JarFile(jar)) {
            var entry = archive.getEntry("plugin.yml");
            if (entry == null) {
                System.out.println("[host] " + jar.getName() + ": no plugin.yml");
                return false;
            }
            try (InputStream stream = archive.getInputStream(entry)) {
                descriptor = Descriptor.read(new String(stream.readAllBytes(), StandardCharsets.UTF_8));
            }
        }
        if (descriptor.main == null) {
            System.out.println("[host] " + jar.getName() + ": plugin.yml names no main class");
            return false;
        }

        URLClassLoader loader = new URLClassLoader(
            new URL[] {jar.toURI().toURL()}, PluginHost.class.getClassLoader());
        Class<?> type = Class.forName(descriptor.main, true, loader);
        Object instance = type.getDeclaredConstructor().newInstance();
        if (!(instance instanceof JavaPlugin plugin)) {
            System.out.println("[host] " + descriptor.name + ": main class is not a JavaPlugin");
            return false;
        }

        File dataFolder = new File(jar.getParentFile(), descriptor.name);
        plugin.init(
            org.bukkit.Bukkit.getServer(),
            new PluginDescriptionFile(descriptor.name, descriptor.version, descriptor.main),
            dataFolder,
            descriptor.commands.toArray(new String[0]));
        plugin.onEnable();
        plugin.setEnabled(true);
        loaded.add(plugin);
        System.out.println("[host] enabled " + plugin.getDescription().getFullName());
        return true;
    }

    /** Disables everything, newest first. */
    public static void disableAll() {
        for (int i = loaded.size() - 1; i >= 0; i--) {
            Plugin plugin = loaded.get(i);
            try {
                plugin.onDisable();
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed to disable: " + error);
            }
        }
        loaded.clear();
    }

    /** The few plugin.yml keys the host needs before a real YAML reader exists. */
    private static final class Descriptor {
        String name = "unknown";
        String version = "0";
        String main;
        final List<String> commands = new ArrayList<>();

        static Descriptor read(String text) {
            Descriptor out = new Descriptor();
            boolean inCommands = false;
            for (String raw : text.split("\n")) {
                String line = raw.stripTrailing();
                if (line.isBlank() || line.stripLeading().startsWith("#")) {
                    continue;
                }
                int indent = line.length() - line.stripLeading().length();
                String trimmed = line.strip();
                if (indent == 0) {
                    inCommands = trimmed.startsWith("commands:");
                    int colon = trimmed.indexOf(':');
                    if (colon < 0) {
                        continue;
                    }
                    String key = trimmed.substring(0, colon).strip();
                    String value = unquote(trimmed.substring(colon + 1).strip());
                    switch (key) {
                        case "name" -> out.name = value;
                        case "version" -> out.version = value;
                        case "main" -> out.main = value;
                        default -> { }
                    }
                } else if (inCommands && indent == 2 && trimmed.endsWith(":")) {
                    out.commands.add(trimmed.substring(0, trimmed.length() - 1).strip());
                }
            }
            return out;
        }

        private static String unquote(String value) {
            if (value.length() >= 2
                && ((value.startsWith("'") && value.endsWith("'"))
                    || (value.startsWith("\"") && value.endsWith("\"")))) {
                return value.substring(1, value.length() - 1);
            }
            return value;
        }
    }
}
