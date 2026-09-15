package foton;

import java.io.File;
import java.io.InputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.net.URLConnection;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
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
    private static final Map<String, org.bukkit.plugin.java.PluginClassLoader> pluginLoaders = new HashMap<>();
    private static final Map<String, PluginState> states = new HashMap<>();
    private static final Map<Plugin, PluginState> statesByPlugin =
        new java.util.IdentityHashMap<>();

    private enum Status { DISCOVERED, LOADING, LOADED, ENABLING, ENABLED, FAILED, DISABLED }

    private static final class PluginState {
        final PluginDescriptionFile descriptor;
        org.bukkit.plugin.java.PluginClassLoader loader;
        JavaPlugin plugin;
        Status status = Status.DISCOVERED;
        boolean enableAttempted;
        boolean cleaned;

        PluginState(PluginDescriptionFile descriptor) {
            this.descriptor = descriptor;
        }
    }

    private PluginHost() {}

    /** Loads and enables every plugin in a directory. Returns how many worked. */
    public static int loadAll(String directory) {
        loadAllOnLoad(directory);
        return enableAll();
    }

    /** Discovers and loads plugins, invoking only their onLoad lifecycle phase. */
    public static int loadAllOnLoad(String directory) {
        ensureServer();
        List<File> ordered = orderedJars(new File(directory));
        int loadedNow = 0;
        for (File jar : ordered) {
            try {
                if (load(jar, false)) loadedNow++;
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        return loadedNow;
    }

    /** Enables every plugin successfully loaded by loadAllOnLoad. */
    public static int enableAll() {
        int enabledNow = 0;
        for (Plugin plugin : new ArrayList<>(loaded)) {
            if (plugin.isEnabled()) continue;
            PluginState state = stateOf(plugin);
            String unavailable = unavailableDependency(plugin.getDescription(), true);
            if (unavailable != null) {
                System.out.println("[host] " + plugin.getName()
                    + ": required dependency " + unavailable + " did not enable");
                if (state != null) state.status = Status.FAILED;
                cleanup(plugin);
                continue;
            }
            try {
                if (!(plugin instanceof JavaPlugin java)) continue;
                enable(java);
                enabledNow++;
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                if (state != null) state.status = Status.FAILED;
                cleanup(plugin);
            }
        }
        return enabledNow;
    }

    private static void ensureServer() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(new FotonServer());
        }
    }

    private static List<File> orderedJars(File dir) {
        File[] jars = dir.listFiles((d, name) -> name.endsWith(".jar"));
        if (jars == null) {
            System.out.println("[host] no plugin directory at " + dir);
            return List.of();
        }
        Map<String, File> jarsByName = new HashMap<>();
        Map<String, PluginDescriptionFile> descriptors = new HashMap<>();
        for (File jar : jars) {
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                String key = descriptor.getName().toLowerCase(java.util.Locale.ROOT);
                if (jarsByName.putIfAbsent(key, jar) != null) {
                    System.out.println("[host] duplicate plugin name " + descriptor.getName() + "; skipping " + jar.getName());
                    continue;
                }
                descriptors.put(key, descriptor);
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        List<File> ordered = new ArrayList<>();
        Map<String, Integer> states = new HashMap<>();
        Set<String> failed = new HashSet<>();
        for (String key : new java.util.TreeSet<>(jarsByName.keySet())) {
            order(key, jarsByName, descriptors, states, failed, ordered);
        }
        return ordered;
    }

    private static boolean order(String key, Map<String, File> jars,
                                 Map<String, PluginDescriptionFile> descriptors,
                                 Map<String, Integer> states, Set<String> failed,
                                 List<File> ordered) {
        Integer state = states.get(key);
        if (state != null) {
            if (state == 1) {
                System.out.println("[host] dependency cycle involving " + descriptors.get(key).getName());
                failed.add(key);
                return false;
            }
            return !failed.contains(key);
        }
        states.put(key, 1);
        PluginDescriptionFile descriptor = descriptors.get(key);
        boolean valid = descriptor != null;
        if (descriptor != null) {
            for (String dependency : descriptor.getDepend()) {
                String dep = dependency.toLowerCase(java.util.Locale.ROOT);
                if (!jars.containsKey(dep)) {
                    System.out.println("[host] " + descriptor.getName() + ": missing required dependency " + dependency);
                    valid = false;
                } else if (!order(dep, jars, descriptors, states, failed, ordered)) {
                    valid = false;
                }
            }
            for (String dependency : descriptor.getSoftDepend()) {
                String dep = dependency.toLowerCase(java.util.Locale.ROOT);
                if (jars.containsKey(dep)) order(dep, jars, descriptors, states, failed, ordered);
            }
        }
        states.put(key, 2);
        if (!valid) {
            failed.add(key);
            return false;
        }
        ordered.add(jars.get(key));
        return true;
    }

    private static PluginDescriptionFile readDescriptor(File jar) throws Exception {
        try (JarFile archive = new JarFile(jar)) {
            // Paper plugins may omit the legacy descriptor.
            for (String descriptorName : new String[] {"plugin.yml", "paper-plugin.yml"}) {
                var entry = archive.getEntry(descriptorName);
                if (entry == null) continue;
                try (InputStream stream = archive.getInputStream(entry)) {
                    return new PluginDescriptionFile(stream);
                }
            }
            throw new InvalidDescriptionException("no plugin.yml or paper-plugin.yml");
        }
    }

    private static boolean load(File jar, boolean enableNow) throws Throwable {
        PluginDescriptionFile descriptor = readDescriptor(jar);
        if (byName(descriptor.getName()) != null) {
            System.out.println("[host] " + descriptor.getName() + " is already enabled; skipping " + jar.getName());
            return false;
        }
        String key = key(descriptor.getName());
        PluginState state = new PluginState(descriptor);
        states.put(key, state);
        String unavailable = unavailableDependency(descriptor, false);
        if (unavailable != null) {
            System.out.println("[host] " + descriptor.getName()
                + ": required dependency " + unavailable + " did not load");
            rollback(state);
            return false;
        }

        org.bukkit.plugin.java.PluginClassLoader loader = null;
        try {
            loader = new org.bukkit.plugin.java.PluginClassLoader(
                pluginUrls(jar), dependencyParent(descriptor));
            state.loader = loader;
            File dataFolder = new File(jar.getParentFile(), descriptor.getName());
            // Before the constructor, not after: a plugin may call getName()
            // or getLogger() from it, and several do.
            loader.describe(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            Class<?> type = Class.forName(descriptor.getMain(), true, loader);
            Object instance = type.getDeclaredConstructor().newInstance();
            if (!(instance instanceof JavaPlugin candidate)) {
                System.out.println(
                    "[host] " + descriptor.getName() + ": main class is not a JavaPlugin");
                rollback(state);
                return false;
            }
            state.plugin = candidate;
            statesByPlugin.put(candidate, state);

            loader.setPlugin(candidate);
            candidate.init(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            FotonScheduler.activatePlugin(candidate);
            loaded.add(candidate);
            pluginLoaders.put(descriptor.getName().toLowerCase(java.util.Locale.ROOT), loader);
            state.status = Status.LOADING;
            candidate.onLoad();
            state.status = Status.LOADED;
            if (enableNow) enable(candidate);
            return true;
        } catch (Throwable error) {
            if (state.plugin == null && loader != null) {
                state.plugin = loader.getPlugin();
                if (state.plugin != null) statesByPlugin.put(state.plugin, state);
            }
            rollback(state);
            throw error;
        }
    }

    private static void rollback(PluginState state) {
        state.status = Status.FAILED;
        if (state.plugin != null) {
            cleanup(state.plugin);
            return;
        }
        state.cleaned = true;
        if (state.loader != null) close(state.loader, state.descriptor.getName());
        state.loader = null;
    }

    private static String unavailableDependency(
            PluginDescriptionFile descriptor, boolean enabling) {
        for (String dependency : descriptor.getDepend()) {
            PluginState state = states.get(key(dependency));
            boolean available = state != null && (enabling
                ? state.status == Status.ENABLED
                : state.status == Status.LOADED || state.status == Status.ENABLED);
            if (!available) return dependency;
        }
        return null;
    }

    private static URL[] pluginUrls(File jar) throws Exception {
        List<URL> urls = new ArrayList<>();
        urls.add(jar.toURI().toURL());
        File libraries = new File(jar.getParentFile(), ".foton-libraries");
        try (JarFile archive = new JarFile(jar)) {
            var entry = archive.getEntry("paper-libraries.json");
            if (entry == null) return urls.toArray(new URL[0]);
            String json;
            try (InputStream stream = archive.getInputStream(entry)) { json = new String(stream.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8); }
            int serializationIndex = json.indexOf("kotlinx-serialization-json:");
            if (serializationIndex >= 0) {
                int versionStart = serializationIndex + "kotlinx-serialization-json:".length();
                int versionEnd = json.indexOf("\"", versionStart);
                if (versionEnd > versionStart) json += "\"org.jetbrains.kotlinx:kotlinx-serialization-core:" + json.substring(versionStart, versionEnd) + "\"";
            }
            java.util.regex.Matcher matcher = java.util.regex.Pattern.compile("\"([A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+:[A-Za-z0-9_.-]+)\"").matcher(json);
            while (matcher.find()) {
                String coordinate = matcher.group(1);
                String[] parts = coordinate.split(":", 3);
                String artifact = parts[1];
                // Select the JVM variant for Kotlin Multiplatform artifacts.
                if (parts[0].startsWith("org.jetbrains.kotlinx") && !artifact.endsWith("-jvm")) artifact += "-jvm";
                Path target = libraries.toPath().resolve(parts[0].replace(".", "/")).resolve(artifact).resolve(parts[2]).resolve(artifact+"-"+parts[2]+".jar");
                if (!Files.exists(target)) {
                    Files.createDirectories(target.getParent());
                    URLConnection connection = new java.net.URL("https://repo.maven.apache.org/maven2/"+parts[0].replace(".", "/")+"/"+artifact+"/"+parts[2]+"/"+artifact+"-"+parts[2]+".jar").openConnection();
                    connection.setConnectTimeout(10_000); connection.setReadTimeout(30_000);
                    try (InputStream input = connection.getInputStream()) { Files.copy(input, target, StandardCopyOption.REPLACE_EXISTING); }
                }
                urls.add(target.toUri().toURL());
            }
        }
        return urls.toArray(new URL[0]);
    }

    private static ClassLoader dependencyParent(PluginDescriptionFile descriptor) {
        List<ClassLoader> dependencies = new ArrayList<>();
        for (String name : descriptor.getDepend()) {
            org.bukkit.plugin.java.PluginClassLoader loader = pluginLoaders.get(name.toLowerCase(java.util.Locale.ROOT));
            if (loader != null) dependencies.add(loader);
        }
        for (String name : descriptor.getSoftDepend()) {
            org.bukkit.plugin.java.PluginClassLoader loader = pluginLoaders.get(name.toLowerCase(java.util.Locale.ROOT));
            if (loader != null && !dependencies.contains(loader)) dependencies.add(loader);
        }
        return dependencies.isEmpty() ? PluginHost.class.getClassLoader() : new DependencyClassLoader(dependencies);
    }

    private static final class DependencyClassLoader extends ClassLoader {
        private final List<ClassLoader> dependencies;

        private DependencyClassLoader(List<ClassLoader> dependencies) {
            super(PluginHost.class.getClassLoader());
            this.dependencies = List.copyOf(dependencies);
        }

        @Override
        protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
            try {
                return super.loadClass(name, resolve);
            } catch (ClassNotFoundException missingFromServer) {
                for (ClassLoader dependency : dependencies) {
                    try {
                        return Class.forName(name, false, dependency);
                    } catch (ClassNotFoundException ignored) {
                        // Try the next declared dependency.
                    }
                }
                throw missingFromServer;
            }
        }
    }

    private static void enable(JavaPlugin plugin) {
        PluginState state = stateOf(plugin);
        if (state != null) {
            state.enableAttempted = true;
            state.status = Status.ENABLING;
        }
        plugin.setEnabled(true);
        plugin.onEnable();
        FotonLifecycle.dispatchCommands(plugin);
        EventBridge.dispatch(new org.bukkit.event.server.PluginEnableEvent(plugin));
        if (state != null) state.status = Status.ENABLED;
        System.out.println("[host] enabled " + plugin.getDescription().getFullName());
    }

    /** Disables one plugin, and lets go of what it claimed. */
    public static void disable(Plugin plugin) {
        cleanup(plugin);
    }

    /** Idempotently rolls back or disables every resource owned by a plugin. */
    public static void cleanup(Plugin plugin) {
        if (plugin == null) return;
        PluginState state = stateOf(plugin);
        if (state == null) return;
        synchronized (state) {
            if (state.cleaned) return;
            FotonScheduler.beginPluginCleanup(plugin);
            state.cleaned = true;
        }

        if (plugin instanceof org.bukkit.plugin.java.JavaPlugin java) {
            java.setEnabled(false);
        }
        loaded.remove(plugin);
        boolean disable = state.enableAttempted;
        if (disable) {
            try {
                plugin.onDisable();
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName()
                    + " failed to disable: " + error);
            }
        }
        release(plugin, "scheduler tasks",
            () -> org.bukkit.Bukkit.getScheduler().cancelTasks(plugin));
        release(plugin, "commands", () -> CommandMap.forget(plugin));
        release(plugin, "services",
            () -> org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin));
        release(plugin, "incoming channels",
            () -> org.bukkit.Bukkit.getMessenger().unregisterIncomingPluginChannel(plugin));
        release(plugin, "outgoing channels",
            () -> org.bukkit.Bukkit.getMessenger().unregisterOutgoingPluginChannel(plugin));
        release(plugin, "listeners", () -> EventBridge.unregister(plugin));

        org.bukkit.plugin.java.PluginClassLoader loader = state.loader;
        if (loader != null) {
            pluginLoaders.remove(key(plugin.getName()), loader);
            close(loader, plugin.getName());
        }
        if (state.status != Status.FAILED) {
            state.status = Status.DISABLED;
        }
        if (disable) {
            release(plugin, "disable event",
                () -> EventBridge.dispatch(
                    new org.bukkit.event.server.PluginDisableEvent(plugin)));
        }
        state.plugin = null;
        state.loader = null;
        statesByPlugin.remove(plugin);
    }

    private static void release(Plugin plugin, String resource, Runnable action) {
        try {
            action.run();
        } catch (Throwable error) {
            System.out.println("[host] " + plugin.getName() + " failed to release "
                + resource + ": " + error);
        }
    }

    private static void close(
            org.bukkit.plugin.java.PluginClassLoader loader, String pluginName) {
        try {
            loader.close();
        } catch (java.io.IOException error) {
            System.out.println("[host] " + pluginName
                + " classloader failed to close: " + error);
        }
    }

    private static PluginState stateOf(Plugin plugin) {
        return statesByPlugin.get(plugin);
    }

    static int lifecycleReferenceCount(String pluginName) {
        int references = pluginLoaders.containsKey(key(pluginName)) ? 1 : 0;
        PluginState state = states.get(key(pluginName));
        if (state != null && state.plugin != null) references++;
        if (state != null && state.loader != null) references++;
        for (Plugin plugin : statesByPlugin.keySet()) {
            if (plugin.getName().equals(pluginName)) references++;
        }
        for (Plugin plugin : loaded) {
            if (plugin.getName().equals(pluginName)) references++;
        }
        return references;
    }

    private static String key(String name) {
        return name.toLowerCase(java.util.Locale.ROOT);
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
