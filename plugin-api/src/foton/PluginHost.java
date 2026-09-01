package foton;

import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.util.ArrayList;
import java.util.List;
import java.util.jar.JarFile;
import org.bukkit.plugin.Plugin;
import org.bukkit.plugin.InvalidDescriptionException;
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
        PluginDescriptionFile descriptor;
        try (JarFile archive = new JarFile(jar)) {
            var entry = archive.getEntry("plugin.yml");
            if (entry == null) {
                System.out.println("[host] " + jar.getName() + ": no plugin.yml");
                return false;
            }
            try (InputStream stream = archive.getInputStream(entry)) {
                descriptor = new PluginDescriptionFile(stream);
            } catch (InvalidDescriptionException bad) {
                System.out.println("[host] " + jar.getName() + ": " + bad.getMessage());
                return false;
            }
        }

        URLClassLoader loader = new URLClassLoader(
            new URL[] {jar.toURI().toURL()}, PluginHost.class.getClassLoader());
        Class<?> type = Class.forName(descriptor.getMain(), true, loader);
        Object instance = type.getDeclaredConstructor().newInstance();
        if (!(instance instanceof JavaPlugin plugin)) {
            System.out.println(
                "[host] " + descriptor.getName() + ": main class is not a JavaPlugin");
            return false;
        }

        File dataFolder = new File(jar.getParentFile(), descriptor.getName());
        plugin.init(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
        plugin.onEnable();
        plugin.setEnabled(true);
        loaded.add(plugin);
        System.out.println("[host] enabled " + plugin.getDescription().getFullName());
        return true;
    }

    /** Disables one plugin, and lets go of what it claimed. */
    public static void disable(Plugin plugin) {
        if (plugin == null || !loaded.remove(plugin)) {
            return;
        }
        CommandMap.forget(plugin);
        org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin);
        EventBridge.unregister(plugin);
        try {
            plugin.onDisable();
        } catch (Throwable error) {
            System.out.println("[host] " + plugin.getName() + " failed to disable: " + error);
        }
        if (plugin instanceof org.bukkit.plugin.java.JavaPlugin java) {
            java.setEnabled(false);
        }
        EventBridge.dispatch(new org.bukkit.event.server.PluginDisableEvent(plugin));
    }

    /** The plugin with this name, or null. Case-insensitive, as Bukkit is. */
    public static Plugin byName(String name) {
        if (name == null) {
            return null;
        }
        for (Plugin plugin : loaded) {
            if (plugin.getName().equalsIgnoreCase(name)) {
                return plugin;
            }
        }
        return null;
    }

    /** Everything enabled, in the order it was enabled. */
    public static Plugin[] all() {
        return loaded.toArray(new Plugin[0]);
    }

    /** Disables everything, newest first. */
    public static void disableAll() {
        while (!loaded.isEmpty()) {
            disable(loaded.get(loaded.size() - 1));
        }
    }
}
