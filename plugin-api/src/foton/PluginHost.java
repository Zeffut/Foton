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
import java.util.Collection;
import java.util.Comparator;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.PriorityQueue;
import java.util.Set;
import java.util.TreeMap;
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
    private static final Comparator<DiscoveredPlugin> PLUGIN_ORDER =
        Comparator.comparing((DiscoveredPlugin plugin) -> plugin.descriptor.getName(),
            String.CASE_INSENSITIVE_ORDER)
            .thenComparing(plugin -> plugin.descriptor.getName())
            .thenComparing(plugin -> plugin.jar.getName());

    private enum Status { DISCOVERED, LOADING, LOADED, ENABLING, ENABLED, FAILED, DISABLED }

    private static final class PluginState {
        final PluginDescriptionFile descriptor;
        org.bukkit.plugin.java.PluginClassLoader loader;
        JavaPlugin plugin;
        Status status = Status.DISCOVERED;
        boolean enableAttempted;
        final java.util.concurrent.atomic.AtomicBoolean cleaned =
            new java.util.concurrent.atomic.AtomicBoolean();

        PluginState(PluginDescriptionFile descriptor) {
            this.descriptor = descriptor;
        }
    }

    private static final class DiscoveredPlugin {
        final File jar;
        final PluginDescriptionFile descriptor;
        final String key;

        DiscoveredPlugin(File jar, PluginDescriptionFile descriptor) {
            this.jar = jar;
            this.descriptor = descriptor;
            this.key = key(descriptor.getName());
        }
    }

    private static final class DependencyEdge {
        final DiscoveredPlugin from;
        final DiscoveredPlugin to;
        boolean required;
        boolean removed;

        DependencyEdge(DiscoveredPlugin from, DiscoveredPlugin to, boolean required) {
            this.from = from;
            this.to = to;
            this.required = required;
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
        List<DiscoveredPlugin> ordered = orderedPlugins(new File(directory));
        for (DiscoveredPlugin plugin : ordered) {
            try {
                load(plugin, false);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        propagateRequiredFailures(false);
        int loadedNow = 0;
        for (DiscoveredPlugin plugin : ordered) {
            PluginState state = states.get(plugin.key);
            if (state != null && state.descriptor == plugin.descriptor
                    && (state.status == Status.LOADED || state.status == Status.ENABLED)) {
                loadedNow++;
            }
        }
        return loadedNow;
    }

    /** Enables every plugin successfully loaded by loadAllOnLoad. */
    public static int enableAll() {
        Set<Plugin> enabledNow = java.util.Collections.newSetFromMap(
            new java.util.IdentityHashMap<>());
        for (Plugin plugin : new ArrayList<>(loaded)) {
            if (plugin.isEnabled()) continue;
            PluginState state = stateOf(plugin);
            String unavailable = unavailablePriorDependency(plugin.getDescription(), true);
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
                enabledNow.add(plugin);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                if (state != null) state.status = Status.FAILED;
                cleanup(plugin);
            }
        }
        propagateRequiredFailures(true);
        enabledNow.removeIf(plugin -> !plugin.isEnabled() || !loaded.contains(plugin));
        return enabledNow.size();
    }

    private static void ensureServer() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(new FotonServer());
        }
    }

    private static List<DiscoveredPlugin> orderedPlugins(File dir) {
        File[] jars = dir.listFiles((d, name) -> name.endsWith(".jar"));
        if (jars == null) {
            System.out.println("[host] no plugin directory at " + dir);
            return List.of();
        }
        java.util.Arrays.sort(jars, Comparator.comparing(File::getName));
        List<DiscoveredPlugin> discovered = new ArrayList<>();
        for (File jar : jars) {
            try {
                discovered.add(new DiscoveredPlugin(jar, readDescriptor(jar)));
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        discovered.sort(PLUGIN_ORDER);
        return order(discovered);
    }

    private static List<DiscoveredPlugin> order(List<DiscoveredPlugin> discovered) {
        Set<DiscoveredPlugin> rejected = new HashSet<>();
        rejectDuplicateNames(discovered, rejected);
        rejectDuplicateAliases(discovered, rejected);
        Map<String, DiscoveredPlugin> identities = identities(discovered, rejected);
        rejectMissingRequired(discovered, identities, rejected);
        propagateRequiredRejections(discovered, identities, rejected);

        List<DependencyEdge> edges = dependencyEdges(discovered, identities, rejected);
        rejectRequiredCycles(discovered, edges, rejected);
        propagateRequiredRejections(discovered, identities, rejected);
        breakOptionalCycles(discovered, edges, rejected);
        return topologicalOrder(discovered, edges, rejected);
    }

    private static void rejectDuplicateNames(
            List<DiscoveredPlugin> discovered, Set<DiscoveredPlugin> rejected) {
        Map<String, List<DiscoveredPlugin>> names = new TreeMap<>();
        for (DiscoveredPlugin plugin : discovered) {
            names.computeIfAbsent(plugin.key, ignored -> new ArrayList<>()).add(plugin);
        }
        for (List<DiscoveredPlugin> owners : names.values()) {
            if (owners.size() < 2) continue;
            owners.sort(PLUGIN_ORDER);
            System.out.println("[host] duplicate plugin name: " + describe(owners));
            rejected.addAll(owners);
        }
    }

    private static void rejectDuplicateAliases(
            List<DiscoveredPlugin> discovered, Set<DiscoveredPlugin> rejected) {
        Map<String, List<DiscoveredPlugin>> identities = new TreeMap<>();
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            identities.computeIfAbsent(plugin.key, ignored -> new ArrayList<>()).add(plugin);
            for (String alias : plugin.descriptor.getProvides()) {
                identities.computeIfAbsent(key(alias), ignored -> new ArrayList<>()).add(plugin);
            }
        }
        for (Map.Entry<String, List<DiscoveredPlugin>> entry : identities.entrySet()) {
            List<DiscoveredPlugin> owners = entry.getValue();
            if (owners.size() < 2) continue;
            owners.sort(PLUGIN_ORDER);
            System.out.println("[host] duplicate alias " + entry.getKey() + ": "
                + describe(owners));
            rejected.addAll(owners);
        }
    }

    private static Map<String, DiscoveredPlugin> identities(
            List<DiscoveredPlugin> discovered, Set<DiscoveredPlugin> rejected) {
        Map<String, DiscoveredPlugin> identities = new HashMap<>();
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            identities.put(plugin.key, plugin);
            for (String alias : plugin.descriptor.getProvides()) {
                identities.put(key(alias), plugin);
            }
        }
        return identities;
    }

    private static void rejectMissingRequired(
            List<DiscoveredPlugin> discovered,
            Map<String, DiscoveredPlugin> identities,
            Set<DiscoveredPlugin> rejected) {
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            for (String dependency : requiredDependencies(plugin.descriptor)) {
                if (identities.containsKey(key(dependency))) continue;
                System.out.println("[host] " + plugin.descriptor.getName()
                    + ": missing required dependency " + dependency);
                rejected.add(plugin);
                break;
            }
        }
    }

    private static void propagateRequiredRejections(
            List<DiscoveredPlugin> discovered,
            Map<String, DiscoveredPlugin> identities,
            Set<DiscoveredPlugin> rejected) {
        boolean changed;
        do {
            changed = false;
            for (DiscoveredPlugin plugin : discovered) {
                if (rejected.contains(plugin)) continue;
                for (String dependency : requiredDependencies(plugin.descriptor)) {
                    DiscoveredPlugin provider = identities.get(key(dependency));
                    if (provider == null || !rejected.contains(provider)) continue;
                    System.out.println("[host] " + plugin.descriptor.getName()
                        + ": required dependency " + dependency + " was rejected");
                    rejected.add(plugin);
                    changed = true;
                    break;
                }
            }
        } while (changed);
    }

    private static List<String> requiredDependencies(PluginDescriptionFile descriptor) {
        Map<String, String> dependencies = new LinkedHashMap<>();
        for (String dependency : descriptor.getDepend()) {
            dependencies.put(key(dependency), dependency);
        }
        addRequired(dependencies, descriptor.getBootstrapDependencies());
        addRequired(dependencies, descriptor.getServerDependencies());
        return List.copyOf(dependencies.values());
    }

    private static void addRequired(
            Map<String, String> out, Map<String, PluginDescriptionFile.Dependency> dependencies) {
        for (Map.Entry<String, PluginDescriptionFile.Dependency> entry : dependencies.entrySet()) {
            if (entry.getValue().required()) out.putIfAbsent(key(entry.getKey()), entry.getKey());
        }
    }

    private static List<DependencyEdge> dependencyEdges(
            List<DiscoveredPlugin> discovered,
            Map<String, DiscoveredPlugin> identities,
            Set<DiscoveredPlugin> rejected) {
        Map<String, DependencyEdge> edges = new LinkedHashMap<>();
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            for (String dependency : plugin.descriptor.getDepend()) {
                addEdge(edges, identities.get(key(dependency)), plugin, true);
            }
            for (String dependency : plugin.descriptor.getSoftDepend()) {
                addEdge(edges, identities.get(key(dependency)), plugin, false);
            }
            for (String dependency : plugin.descriptor.getLoadBefore()) {
                addEdge(edges, plugin, identities.get(key(dependency)), false);
            }
            addPaperEdges(edges, plugin, plugin.descriptor.getBootstrapDependencies(), identities);
            addPaperEdges(edges, plugin, plugin.descriptor.getServerDependencies(), identities);
        }
        return new ArrayList<>(edges.values());
    }

    private static void addPaperEdges(
            Map<String, DependencyEdge> edges,
            DiscoveredPlugin plugin,
            Map<String, PluginDescriptionFile.Dependency> dependencies,
            Map<String, DiscoveredPlugin> identities) {
        for (Map.Entry<String, PluginDescriptionFile.Dependency> entry : dependencies.entrySet()) {
            DiscoveredPlugin dependency = identities.get(key(entry.getKey()));
            if (entry.getValue().load() == PluginDescriptionFile.Load.BEFORE) {
                addEdge(edges, dependency, plugin, entry.getValue().required());
            } else if (entry.getValue().load() == PluginDescriptionFile.Load.AFTER) {
                addEdge(edges, plugin, dependency, entry.getValue().required());
            }
        }
    }

    private static void addEdge(
            Map<String, DependencyEdge> edges,
            DiscoveredPlugin from, DiscoveredPlugin to, boolean required) {
        if (from == null || to == null) return;
        String edgeKey = from.key + "\0" + to.key;
        DependencyEdge existing = edges.get(edgeKey);
        if (existing == null) {
            edges.put(edgeKey, new DependencyEdge(from, to, required));
        } else if (required) {
            existing.required = true;
        }
    }

    private static void rejectRequiredCycles(
            List<DiscoveredPlugin> discovered,
            List<DependencyEdge> edges,
            Set<DiscoveredPlugin> rejected) {
        for (List<DiscoveredPlugin> component : components(discovered, edges, rejected)) {
            if (!isCycle(component, edges)) continue;
            boolean required = false;
            for (DependencyEdge edge : edges) {
                if (component.contains(edge.from) && component.contains(edge.to)
                        && edge.required) {
                    required = true;
                    break;
                }
            }
            if (!required) continue;
            component.sort(PLUGIN_ORDER);
            System.out.println("[host] required dependency cycle: " + describe(component));
            rejected.addAll(component);
        }
    }

    private static void breakOptionalCycles(
            List<DiscoveredPlugin> discovered,
            List<DependencyEdge> edges,
            Set<DiscoveredPlugin> rejected) {
        for (List<DiscoveredPlugin> component : components(discovered, edges, rejected)) {
            if (!isCycle(component, edges)) continue;
            for (DependencyEdge edge : edges) {
                if (!component.contains(edge.from) || !component.contains(edge.to)) continue;
                if (PLUGIN_ORDER.compare(edge.from, edge.to) < 0) continue;
                edge.removed = true;
                System.out.println("[host] optional dependency cycle: dropping edge "
                    + edge.from.descriptor.getName() + " -> " + edge.to.descriptor.getName());
            }
        }
    }

    private static List<List<DiscoveredPlugin>> components(
            List<DiscoveredPlugin> discovered,
            List<DependencyEdge> edges,
            Set<DiscoveredPlugin> rejected) {
        List<List<DiscoveredPlugin>> components = new ArrayList<>();
        Set<DiscoveredPlugin> assigned = new HashSet<>();
        for (DiscoveredPlugin seed : discovered) {
            if (rejected.contains(seed) || assigned.contains(seed)) continue;
            Set<DiscoveredPlugin> forward = reachable(seed, edges, rejected, false);
            List<DiscoveredPlugin> component = new ArrayList<>();
            for (DiscoveredPlugin candidate : discovered) {
                if (rejected.contains(candidate) || !forward.contains(candidate)) continue;
                if (reachable(candidate, edges, rejected, false).contains(seed)) {
                    component.add(candidate);
                }
            }
            assigned.addAll(component);
            components.add(component);
        }
        return components;
    }

    private static Set<DiscoveredPlugin> reachable(
            DiscoveredPlugin start,
            List<DependencyEdge> edges,
            Set<DiscoveredPlugin> rejected,
            boolean honorRemoved) {
        Set<DiscoveredPlugin> found = new HashSet<>();
        List<DiscoveredPlugin> pending = new ArrayList<>();
        pending.add(start);
        while (!pending.isEmpty()) {
            DiscoveredPlugin current = pending.remove(pending.size() - 1);
            if (!found.add(current)) continue;
            for (DependencyEdge edge : edges) {
                if (edge.from != current || rejected.contains(edge.to)) continue;
                if (honorRemoved && edge.removed) continue;
                pending.add(edge.to);
            }
        }
        return found;
    }

    private static boolean isCycle(
            List<DiscoveredPlugin> component, List<DependencyEdge> edges) {
        if (component.size() > 1) return true;
        if (component.isEmpty()) return false;
        DiscoveredPlugin only = component.get(0);
        for (DependencyEdge edge : edges) {
            if (edge.from == only && edge.to == only) return true;
        }
        return false;
    }

    private static List<DiscoveredPlugin> topologicalOrder(
            List<DiscoveredPlugin> discovered,
            List<DependencyEdge> edges,
            Set<DiscoveredPlugin> rejected) {
        Map<DiscoveredPlugin, Integer> indegrees = new HashMap<>();
        for (DiscoveredPlugin plugin : discovered) {
            if (!rejected.contains(plugin)) indegrees.put(plugin, 0);
        }
        for (DependencyEdge edge : edges) {
            if (edge.removed || rejected.contains(edge.from) || rejected.contains(edge.to)) {
                continue;
            }
            indegrees.put(edge.to, indegrees.get(edge.to) + 1);
        }
        PriorityQueue<DiscoveredPlugin> ready = new PriorityQueue<>(PLUGIN_ORDER);
        for (Map.Entry<DiscoveredPlugin, Integer> entry : indegrees.entrySet()) {
            if (entry.getValue() == 0) ready.add(entry.getKey());
        }
        List<DiscoveredPlugin> ordered = new ArrayList<>();
        while (!ready.isEmpty()) {
            DiscoveredPlugin current = ready.remove();
            ordered.add(current);
            for (DependencyEdge edge : edges) {
                if (edge.removed || edge.from != current || rejected.contains(edge.to)) continue;
                int remaining = indegrees.compute(edge.to,
                    (ignored, degree) -> degree == null ? 0 : degree - 1);
                if (remaining == 0) ready.add(edge.to);
            }
        }
        return ordered;
    }

    private static String describe(Collection<DiscoveredPlugin> plugins) {
        List<DiscoveredPlugin> ordered = new ArrayList<>(plugins);
        ordered.sort(PLUGIN_ORDER);
        List<String> names = new ArrayList<>();
        for (DiscoveredPlugin plugin : ordered) {
            names.add(plugin.descriptor.getName() + " (" + plugin.jar.getName() + ")");
        }
        return String.join(", ", names);
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

    private static boolean load(DiscoveredPlugin discovered, boolean enableNow) throws Throwable {
        File jar = discovered.jar;
        PluginDescriptionFile descriptor = discovered.descriptor;
        if (byName(descriptor.getName()) != null) {
            System.out.println("[host] " + descriptor.getName() + " is already enabled; skipping " + jar.getName());
            return false;
        }
        PluginState state = new PluginState(descriptor);
        registerState(state);
        String unavailable = unavailablePriorDependency(descriptor, false);
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
            registerLoader(descriptor, loader);
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
        state.cleaned.set(true);
        if (state.loader != null) close(state.loader, state.descriptor.getName());
        state.loader = null;
    }

    private static String unavailablePriorDependency(
            PluginDescriptionFile descriptor, boolean enabling) {
        for (String dependency : descriptor.getDepend()) {
            if (!dependencyAvailable(dependency, enabling)) return dependency;
        }
        String unavailable = unavailablePriorPaperDependency(
            descriptor.getBootstrapDependencies(), enabling);
        if (unavailable != null) return unavailable;
        unavailable = unavailablePriorPaperDependency(
            descriptor.getServerDependencies(), enabling);
        if (unavailable != null) return unavailable;
        return null;
    }

    private static String unavailablePriorPaperDependency(
            Map<String, PluginDescriptionFile.Dependency> dependencies,
            boolean enabling) {
        for (Map.Entry<String, PluginDescriptionFile.Dependency> entry
                : dependencies.entrySet()) {
            PluginDescriptionFile.Dependency dependency = entry.getValue();
            if (dependency.required()
                    && dependency.load() == PluginDescriptionFile.Load.BEFORE
                    && !dependencyAvailable(entry.getKey(), enabling)) {
                return entry.getKey();
            }
        }
        return null;
    }

    private static void propagateRequiredFailures(boolean enabling) {
        boolean changed;
        do {
            changed = false;
            for (Plugin plugin : new ArrayList<>(loaded)) {
                PluginState state = stateOf(plugin);
                if (state == null) continue;
                String unavailable = unavailableRequiredDependency(
                    plugin.getDescription(), enabling);
                if (unavailable == null) continue;
                System.out.println("[host] " + plugin.getName()
                    + ": required dependency " + unavailable
                    + (enabling ? " did not enable" : " did not load"));
                state.status = Status.FAILED;
                cleanup(plugin);
                changed = true;
            }
        } while (changed);
    }

    private static String unavailableRequiredDependency(
            PluginDescriptionFile descriptor, boolean enabling) {
        for (String dependency : requiredDependencies(descriptor)) {
            if (!dependencyAvailable(dependency, enabling)) return dependency;
        }
        return null;
    }

    private static boolean dependencyAvailable(String name, boolean enabling) {
        PluginState state = states.get(key(name));
        return state != null && (enabling
            ? state.status == Status.ENABLED
            : state.status == Status.LOADED || state.status == Status.ENABLED);
    }

    private static void registerState(PluginState state) {
        states.put(key(state.descriptor.getName()), state);
        for (String alias : state.descriptor.getProvides()) {
            states.put(key(alias), state);
        }
    }

    private static void registerLoader(
            PluginDescriptionFile descriptor,
            org.bukkit.plugin.java.PluginClassLoader loader) {
        pluginLoaders.put(key(descriptor.getName()), loader);
        for (String alias : descriptor.getProvides()) {
            pluginLoaders.put(key(alias), loader);
        }
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
        addClasspathDependencies(dependencies, descriptor.getBootstrapDependencies());
        addClasspathDependencies(dependencies, descriptor.getServerDependencies());
        return dependencies.isEmpty() ? PluginHost.class.getClassLoader() : new DependencyClassLoader(dependencies);
    }

    private static void addClasspathDependencies(
            List<ClassLoader> out,
            Map<String, PluginDescriptionFile.Dependency> dependencies) {
        for (Map.Entry<String, PluginDescriptionFile.Dependency> entry
                : dependencies.entrySet()) {
            if (!entry.getValue().joinClasspath()) continue;
            org.bukkit.plugin.java.PluginClassLoader loader = pluginLoaders.get(key(entry.getKey()));
            if (loader != null && !out.contains(loader)) out.add(loader);
        }
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
        FotonScheduler.beginPluginCleanup(plugin);
        if (!state.cleaned.compareAndSet(false, true)) return;

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
            pluginLoaders.entrySet().removeIf(entry -> entry.getValue() == loader);
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
            if (plugin.getName().equalsIgnoreCase(name)
                    || containsIgnoreCase(plugin.getDescription().getProvides(), name)) {
                return plugin;
            }
        }
        return null;
    }

    private static boolean containsIgnoreCase(List<String> values, String wanted) {
        for (String value : values) {
            if (value.equalsIgnoreCase(wanted)) return true;
        }
        return false;
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
