package foton;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
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
import java.util.concurrent.locks.ReentrantLock;
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
    private static final Map<Plugin, org.bukkit.plugin.java.PluginClassLoader> loadersByPlugin =
        new java.util.IdentityHashMap<>();
    private static final Map<Plugin, InvocationState> invocations =
        new java.util.IdentityHashMap<>();
    private static final ThreadLocal<Map<InvocationState, Integer>> threadInvocations =
        ThreadLocal.withInitial(java.util.IdentityHashMap::new);
    private static final ThreadLocal<Integer> lifecycleCallbackDepth =
        ThreadLocal.withInitial(() -> 0);
    private static final Object lifecycle = new Object();
    /** Serializes lifecycle callbacks without holding the state monitor.
     *
     * <p>The lock is reentrant because Bukkit permits a plugin callback to
     * disable that same plugin. State transitions still use {@link #lifecycle}
     * so an in-flight event can drain without needing this operation lock. */
    private static final ReentrantLock lifecycleOperation = new ReentrantLock(true);

    private PluginHost() {}

    /** Marks a real host shutdown; ordinary plugin disable/re-enable does not call this. */
    public static void markStopping() {
        org.bukkit.Bukkit.markStopping();
    }

    static Object lifecycleLock() {
        return lifecycle;
    }

    static void requireEnabled(Plugin plugin, String operation) {
        if (plugin == null) {
            throw new IllegalArgumentException("plugin cannot be null");
        }
        if (!plugin.isEnabled()) {
            throw new IllegalStateException(
                "Plugin attempted to " + operation + " while disabled: " + plugin.getName());
        }
    }

    static Invocation beginInvocation(Plugin plugin) {
        synchronized (lifecycle) {
            InvocationState state = invocations.get(plugin);
            if (state == null || !state.accepting || !plugin.isEnabled()) {
                return null;
            }
            state.active++;
            threadInvocations.get().merge(state, 1, Integer::sum);
            return new Invocation(state);
        }
    }

    /** Loads and enables every plugin in a directory. Returns how many worked. */
    public static int loadAll(String directory) {
        lifecycleOperation.lock();
        try {
            loadAllOnLoadSerialized(directory);
            return enableAllSerialized();
        } finally {
            lifecycleOperation.unlock();
        }
    }

    /**
     * Discovers and loads plugins, invoking only their onLoad lifecycle phase.
     *
     * <p>Construction and publication are deliberately a separate pass from
     * callbacks. Bukkit exposes the complete plugin set by the time any
     * {@code onLoad} runs; ViaVersion relies on that to discover companion
     * plugins such as ViaBackwards during bootstrap.
     */
    public static int loadAllOnLoad(String directory) {
        lifecycleOperation.lock();
        try {
            return loadAllOnLoadSerialized(directory);
        } finally {
            lifecycleOperation.unlock();
        }
    }

    private static int loadAllOnLoadSerialized(String directory) {
        ensureServer();
        List<File> ordered = orderedJars(new File(directory));
        List<JavaPlugin> constructed = new ArrayList<>();
        for (File jar : ordered) {
            try {
                JavaPlugin plugin = constructAndRegister(jar);
                if (plugin != null) constructed.add(plugin);
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }

        int loadedNow = 0;
        for (JavaPlugin plugin : constructed) {
            try {
                String unavailable = unavailableLoadedDependency(plugin);
                if (unavailable != null) {
                    System.out.println("[host] " + plugin.getName()
                        + ": required dependency " + unavailable + " failed to load");
                    synchronized (lifecycle) {
                        InvocationState state = invocations.get(plugin);
                        if (state != null) {
                            state.loading = false;
                            lifecycle.notifyAll();
                        }
                    }
                    discard(plugin);
                    continue;
                }
                boolean discardRequested;
                boolean disableRequested;
                try {
                    invokeLifecycleCallback(plugin::onLoad);
                } finally {
                    synchronized (lifecycle) {
                        InvocationState state = invocations.get(plugin);
                        discardRequested = state != null && state.discardRequested;
                        disableRequested = state != null && state.disableRequested;
                        if (state != null) {
                            state.loading = false;
                            lifecycle.notifyAll();
                        }
                    }
                    if (discardRequested) discardSerialized(plugin);
                    else if (disableRequested) disableSerialized(plugin);
                }
                synchronized (lifecycle) {
                    if (isPublishedLocked(plugin)) loadedNow++;
                }
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                discard(plugin);
            }
        }
        return loadedNow;
    }

    private static List<String> requiredDependencies(Plugin plugin) {
        return requiredDependencies(plugin.getDescription());
    }

    /** Dependencies without which the plugin must not load: Bukkit's
     * {@code depend}, or the {@code required} entries of paper-plugin.yml. */
    private static List<String> requiredDependencies(PluginDescriptionFile descriptor) {
        if (!(descriptor instanceof PaperPluginDescriptor paper)) return descriptor.getDepend();
        List<String> names = new ArrayList<>();
        for (PaperPluginDescriptor.Dependency dependency : paper.paperDependencies()) {
            if (dependency.required() && !names.contains(dependency.name())) {
                names.add(dependency.name());
            }
        }
        return names;
    }

    private static String unavailableLoadedDependency(Plugin plugin) {
        for (String name : requiredDependencies(plugin)) {
            if (byName(name) == null) return name;
        }
        return null;
    }

    /** Enables every plugin successfully loaded by loadAllOnLoad. */
    public static int enableAll() {
        lifecycleOperation.lock();
        try {
            return enableAllSerialized();
        } finally {
            lifecycleOperation.unlock();
        }
    }

    private static int enableAllSerialized() {
        int enabledNow = 0;
        List<Plugin> snapshot;
        synchronized (lifecycle) {
            snapshot = new ArrayList<>(loaded);
        }
        for (Plugin plugin : snapshot) {
            if (plugin.isEnabled()) continue;
            try {
                if (!(plugin instanceof JavaPlugin java)) continue;
                synchronized (lifecycle) {
                    InvocationState state = invocations.get(plugin);
                    if (state == null || state.loading) continue;
                }
                String unavailable = unavailableRequiredDependency(plugin);
                if (unavailable != null) {
                    System.out.println("[host] " + plugin.getName()
                        + ": required dependency " + unavailable + " failed to enable");
                    disable(plugin);
                    continue;
                }
                enable(java);
                enabledNow++;
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                disable(plugin);
            }
        }
        return enabledNow;
    }

    private static String unavailableRequiredDependency(Plugin plugin) {
        for (String name : requiredDependencies(plugin)) {
            Plugin dependency = byName(name);
            if (dependency == null || !dependency.isEnabled()) {
                return name;
            }
        }
        return null;
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
        Set<String> valid = hardDependencyClosure(jarsByName, descriptors);
        Map<String, Set<String>> edges = new HashMap<>();
        for (String key : valid) edges.put(key, new java.util.TreeSet<>());
        for (String key : valid) {
            PluginDescriptionFile descriptor = descriptors.get(key);
            if (descriptor instanceof PaperPluginDescriptor paper) {
                for (PaperPluginDescriptor.Dependency dependency : paper.paperDependencies()) {
                    if (dependency.load() == PaperPluginDescriptor.LoadOrder.BEFORE) {
                        addEdge(edges, pluginKey(dependency.name()), key);
                    } else if (dependency.load() == PaperPluginDescriptor.LoadOrder.AFTER) {
                        addEdge(edges, key, pluginKey(dependency.name()));
                    }
                }
            } else {
                for (String dependency : descriptor.getDepend()) {
                    addEdge(edges, pluginKey(dependency), key);
                }
            }
        }

        List<OrderingEdge> optional = new ArrayList<>();
        for (String key : valid) {
            PluginDescriptionFile descriptor = descriptors.get(key);
            if (!(descriptor instanceof PaperPluginDescriptor)) {
                for (String dependency : descriptor.getSoftDepend()) {
                    optional.add(new OrderingEdge(pluginKey(dependency), key));
                }
                for (String target : descriptor.getLoadBefore()) {
                    optional.add(new OrderingEdge(key, pluginKey(target)));
                }
            }
        }
        optional.sort(java.util.Comparator.comparing(OrderingEdge::from)
            .thenComparing(OrderingEdge::to));
        for (OrderingEdge edge : optional) {
            if (!valid.contains(edge.from()) || !valid.contains(edge.to())
                    || edge.from().equals(edge.to())) {
                continue;
            }
            // Soft dependencies and load-before hints only influence order.
            // Ignore the deterministic edge that would close a cycle; unlike
            // a hard dependency cycle, it must never make a plugin unloadable.
            if (!reachable(edges, edge.to(), edge.from())) {
                addEdge(edges, edge.from(), edge.to());
            }
        }

        List<File> ordered = new ArrayList<>();
        for (String key : topologicalOrder(valid, edges)) {
            ordered.add(jarsByName.get(key));
        }
        return ordered;
    }

    private static Set<String> hardDependencyClosure(Map<String, File> jars,
            Map<String, PluginDescriptionFile> descriptors) {
        Set<String> eligible = new java.util.TreeSet<>(jars.keySet());
        boolean changed;
        do {
            changed = false;
            for (String key : new ArrayList<>(eligible)) {
                PluginDescriptionFile descriptor = descriptors.get(key);
                String unavailable = requiredDependencies(descriptor).stream()
                    .filter(name -> !eligible.contains(pluginKey(name)))
                    .findFirst().orElse(null);
                if (unavailable == null) continue;
                System.out.println("[host] " + descriptor.getName()
                    + ": missing or unavailable required dependency " + unavailable);
                eligible.remove(key);
                changed = true;
            }
        } while (changed);

        Map<String, Set<String>> hardEdges = new HashMap<>();
        for (String key : eligible) hardEdges.put(key, new java.util.TreeSet<>());
        for (String key : eligible) {
            PluginDescriptionFile descriptor = descriptors.get(key);
            if (descriptor instanceof PaperPluginDescriptor paper) {
                for (PaperPluginDescriptor.Dependency dependency : paper.paperDependencies()) {
                    if (dependency.required()
                            && dependency.load() == PaperPluginDescriptor.LoadOrder.BEFORE) {
                        addEdge(hardEdges, pluginKey(dependency.name()), key);
                    } else if (dependency.required()
                            && dependency.load() == PaperPluginDescriptor.LoadOrder.AFTER) {
                        addEdge(hardEdges, key, pluginKey(dependency.name()));
                    }
                }
            } else {
                for (String dependency : descriptor.getDepend()) {
                    addEdge(hardEdges, pluginKey(dependency), key);
                }
            }
        }
        List<String> ordered = topologicalOrder(eligible, hardEdges);
        if (ordered.size() == eligible.size()) return new java.util.TreeSet<>(ordered);

        Set<String> valid = new java.util.TreeSet<>(ordered);
        for (String key : eligible) {
            if (!valid.contains(key)) {
                System.out.println("[host] hard dependency cycle or dependent involving "
                    + descriptors.get(key).getName());
            }
        }
        return valid;
    }

    private static void addEdge(Map<String, Set<String>> edges, String from, String to) {
        Set<String> outgoing = edges.get(from);
        if (outgoing != null && edges.containsKey(to)) outgoing.add(to);
    }

    private static boolean reachable(Map<String, Set<String>> edges, String start,
            String target) {
        if (start.equals(target)) return true;
        Set<String> seen = new HashSet<>();
        java.util.ArrayDeque<String> pending = new java.util.ArrayDeque<>();
        pending.add(start);
        while (!pending.isEmpty()) {
            String current = pending.removeFirst();
            if (!seen.add(current)) continue;
            for (String next : edges.getOrDefault(current, Set.of())) {
                if (next.equals(target)) return true;
                pending.addLast(next);
            }
        }
        return false;
    }

    private static List<String> topologicalOrder(Set<String> nodes,
            Map<String, Set<String>> edges) {
        Map<String, Integer> indegree = new HashMap<>();
        for (String key : nodes) indegree.put(key, 0);
        for (Set<String> outgoing : edges.values()) {
            for (String target : outgoing) indegree.merge(target, 1, Integer::sum);
        }
        java.util.PriorityQueue<String> ready = new java.util.PriorityQueue<>();
        for (Map.Entry<String, Integer> entry : indegree.entrySet()) {
            if (entry.getValue() == 0) ready.add(entry.getKey());
        }
        List<String> ordered = new ArrayList<>();
        while (!ready.isEmpty()) {
            String current = ready.remove();
            ordered.add(current);
            for (String target : edges.getOrDefault(current, Set.of())) {
                int remaining = indegree.merge(target, -1, Integer::sum);
                if (remaining == 0) ready.add(target);
            }
        }
        return ordered;
    }

    private record OrderingEdge(String from, String to) {}

    private static PluginDescriptionFile readDescriptor(File jar) throws Exception {
        try (JarFile archive = new JarFile(jar)) {
            var paper = archive.getEntry("paper-plugin.yml");
            if (paper != null) {
                try (InputStream stream = archive.getInputStream(paper)) {
                    return PaperPluginDescriptor.read(stream);
                }
            }
            var legacy = archive.getEntry("plugin.yml");
            if (legacy != null) {
                try (InputStream stream = archive.getInputStream(legacy)) {
                    return new PluginDescriptionFile(stream);
                }
            }
            throw new InvalidDescriptionException("no plugin.yml or paper-plugin.yml");
        }
    }

    private static JavaPlugin constructAndRegister(File jar) throws Exception {
        PluginDescriptionFile descriptor = readDescriptor(jar);
        if (byName(descriptor.getName()) != null) {
            System.out.println("[host] " + descriptor.getName() + " is already loaded; skipping " + jar.getName());
            return null;
        }
        for (String name : requiredDependencies(descriptor)) {
            if (byName(name) == null) {
                System.out.println("[host] " + descriptor.getName()
                    + ": required dependency " + name + " failed to construct");
                return null;
            }
        }
        org.bukkit.plugin.java.PluginClassLoader loader = new org.bukkit.plugin.java.PluginClassLoader(pluginUrls(jar), dependencyParent(descriptor));
        boolean registered = false;
        JavaPlugin plugin = null;
        try {
            File dataFolder = new File(jar.getParentFile(), descriptor.getName());
            // Before the constructor, not after: a plugin may call getName()
            // or getLogger() from it, and several do.
            loader.describe(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            Class<?> type = Class.forName(descriptor.getMain(), true, loader);
            Object instance = type.getDeclaredConstructor().newInstance();
            if (!(instance instanceof JavaPlugin candidate)) {
                System.out.println(
                    "[host] " + descriptor.getName() + ": main class is not a JavaPlugin");
                return null;
            }
            plugin = candidate;

            loader.setPlugin(plugin);
            plugin.init(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            synchronized (lifecycle) {
                if (findByNameLocked(descriptor.getName()) != null) {
                    System.out.println("[host] " + descriptor.getName()
                        + " was published concurrently; skipping " + jar.getName());
                    return null;
                }
                loaded.add(plugin);
                InvocationState state = new InvocationState();
                state.loading = true;
                invocations.put(plugin, state);
                loadersByPlugin.put(plugin, loader);
                pluginLoaders.put(pluginKey(descriptor.getName()), loader);
            }
            registered = true;
            return plugin;
        } finally {
            if (!registered) {
                if (plugin == null) plugin = loader.getPlugin();
                if (plugin != null) cleanupUnpublished(plugin);
                loader.close();
            }
        }
    }

    private static void cleanupUnpublished(JavaPlugin plugin) {
        plugin.setEnabled(false);
        org.bukkit.Bukkit.getScheduler().cancelTasks(plugin);
        FotonPlayer.removeAttachments(plugin);
        CommandMap.forget(plugin);
        org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterIncomingPluginChannel(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterOutgoingPluginChannel(plugin);
        EventBridge.unregister(plugin);
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
                String repositoryPath = parts[0].replace(".", "/") + "/" + artifact + "/"
                    + parts[2] + "/" + artifact + "-" + parts[2] + ".jar";
                Path libraryRoot = libraries.toPath().toAbsolutePath().normalize();
                Path target = libraryRoot.resolve(repositoryPath).normalize();
                if (!target.startsWith(libraryRoot)) {
                    throw new IOException("Invalid runtime library coordinate: " + coordinate);
                }
                URL source = java.net.URI.create(
                    "https://repo.maven.apache.org/maven2/" + repositoryPath).toURL();
                cacheLibrary(source, target);
                urls.add(target.toUri().toURL());
            }
        }
        return urls.toArray(new URL[0]);
    }

    /** Ensures a cached runtime library is a complete, readable jar. */
    static void cacheLibrary(URL source, Path target) throws IOException {
        if (isUsableLibrary(target)) return;

        Files.createDirectories(target.getParent());
        Path temporary = Files.createTempFile(target.getParent(),
            "." + target.getFileName() + ".", ".part");
        try {
            URLConnection connection = source.openConnection();
            connection.setConnectTimeout(10_000);
            connection.setReadTimeout(30_000);
            try (InputStream input = connection.getInputStream()) {
                Files.copy(input, temporary, StandardCopyOption.REPLACE_EXISTING);
            }
            try {
                validateLibrary(temporary);
            } catch (SecurityException error) {
                throw new IOException("Downloaded library failed jar verification: " + source,
                    error);
            }
            try {
                Files.move(temporary, target, StandardCopyOption.ATOMIC_MOVE,
                    StandardCopyOption.REPLACE_EXISTING);
            } catch (IOException error) {
                // Another downloader may have won the atomic publication race.
                if (!isUsableLibrary(target)) throw error;
            }
        } finally {
            Files.deleteIfExists(temporary);
        }
    }

    private static boolean isUsableLibrary(Path library) {
        try {
            validateLibrary(library);
            return true;
        } catch (IOException | SecurityException invalid) {
            return false;
        }
    }

    private static void validateLibrary(Path library) throws IOException {
        if (!Files.isRegularFile(library)) {
            throw new IOException("Runtime library is not a regular file: " + library);
        }
        boolean hasFile = false;
        try (JarFile archive = new JarFile(library.toFile(), true)) {
            var entries = archive.entries();
            while (entries.hasMoreElements()) {
                var entry = entries.nextElement();
                if (entry.isDirectory()) continue;
                hasFile = true;
                try (InputStream input = archive.getInputStream(entry)) {
                    input.transferTo(java.io.OutputStream.nullOutputStream());
                }
            }
        }
        if (!hasFile) {
            throw new IOException("Runtime library contains no files: " + library);
        }
    }

    private static ClassLoader dependencyParent(PluginDescriptionFile descriptor) {
        List<ClassLoader> dependencies = new ArrayList<>();
        List<String> visible = new ArrayList<>();
        if (descriptor instanceof PaperPluginDescriptor paper) {
            // Paper shares a dependency's classes only when it joins the classpath.
            for (PaperPluginDescriptor.Dependency dependency : paper.paperDependencies()) {
                if (dependency.joinClasspath()) visible.add(dependency.name());
            }
        } else {
            visible.addAll(descriptor.getDepend());
            visible.addAll(descriptor.getSoftDepend());
        }
        synchronized (lifecycle) {
            for (String name : visible) {
                org.bukkit.plugin.java.PluginClassLoader loader =
                    pluginLoaders.get(pluginKey(name));
                if (loader != null && !dependencies.contains(loader)) {
                    dependencies.add(loader);
                }
            }
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
        synchronized (lifecycle) {
            InvocationState state = invocations.get(plugin);
            if (state == null || !isPublishedLocked(plugin)) {
                throw new IllegalStateException("Plugin disappeared while enabling: "
                    + plugin.getName());
            }
            if (state.loading || state.enabling || state.disabling) {
                throw new IllegalStateException("Plugin is already changing lifecycle state: "
                    + plugin.getName());
            }
            state.disableComplete = false;
            state.disableRequested = false;
            state.enabling = true;
            state.accepting = true;
            plugin.setEnabled(true);
        }
        boolean disableRequested;
        boolean discardRequested;
        try {
            invokeLifecycleCallback(plugin::onEnable);
        } finally {
            synchronized (lifecycle) {
                InvocationState state = invocations.get(plugin);
                disableRequested = state != null && state.disableRequested;
                discardRequested = state != null && state.discardRequested;
                if (state != null) {
                    state.enabling = false;
                    lifecycle.notifyAll();
                }
            }
            if (discardRequested) discardSerialized(plugin);
            else if (disableRequested) disableSerialized(plugin);
        }
        synchronized (lifecycle) {
            InvocationState state = invocations.get(plugin);
            if (state == null || !plugin.isEnabled()) {
                throw new IllegalStateException("Plugin disappeared while enabling: "
                    + plugin.getName());
            }
        }
        FotonLifecycle.dispatchCommands(plugin);
        invokeLifecycleCallback(() ->
            EventBridge.dispatch(new org.bukkit.event.server.PluginEnableEvent(plugin)));
        System.out.println("[host] enabled " + plugin.getDescription().getFullName());
    }

    /** Disables one plugin while keeping its Paper-visible loaded identity. */
    public static void disable(Plugin plugin) {
        lifecycleOperation.lock();
        try {
            disableSerialized(plugin);
        } finally {
            lifecycleOperation.unlock();
        }
    }

    private static void disableSerialized(Plugin plugin) {
        final boolean wasEnabled;
        final InvocationState state;
        synchronized (lifecycle) {
            if (plugin == null || !isPublishedLocked(plugin)) return;
            state = invocations.get(plugin);
            if (state == null || state.disabling || state.disableComplete) return;
            if (state.loading || state.enabling) {
                state.disableRequested = true;
                return;
            }
            state.accepting = false;
            state.disabling = true;
            wasEnabled = plugin.isEnabled();
            if (plugin instanceof org.bukkit.plugin.java.JavaPlugin java) {
                java.setEnabled(false);
            }
        }
        // Bukkit calls onDisable while the plugin still owns its listeners,
        // tasks, services, channels, and commands. This lets it release its
        // own state deliberately; the host then guarantees cleanup even when
        // the callback throws.
        if (wasEnabled) {
            try {
                invokeLifecycleCallback(plugin::onDisable);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed to disable: " + error);
            }
        }
        // Paper publishes the event before its manager tears down the plugin's
        // registrations. Other plugins can still discover the disabled plugin
        // and inspect its commands/services/resources from this callback.
        if (wasEnabled) {
            invokeLifecycleCallback(() ->
                EventBridge.dispatch(new org.bukkit.event.server.PluginDisableEvent(plugin)));
        }
        cleanupPublished(plugin);
        awaitInvocations(state, () -> finishNormalDisable(plugin, state));
    }

    private static void cleanupPublished(Plugin plugin) {
        org.bukkit.Bukkit.getScheduler().cancelTasks(plugin);
        FotonPlayer.removeAttachments(plugin);
        CommandMap.forget(plugin);
        org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterIncomingPluginChannel(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterOutgoingPluginChannel(plugin);
        EventBridge.unregister(plugin);
    }

    private static void finishNormalDisable(Plugin plugin, InvocationState state) {
        boolean discardRequested = false;
        synchronized (lifecycle) {
            if (invocations.get(plugin) == state) {
                state.disabling = false;
                state.disableComplete = true;
                discardRequested = state.discardRequested;
            }
            lifecycle.notifyAll();
        }
        if (discardRequested) {
            discard(plugin);
        }
    }

    /** Permanently unpublishes a plugin after a failed load or host shutdown. */
    private static void discard(Plugin plugin) {
        lifecycleOperation.lock();
        try {
            discardSerialized(plugin);
        } finally {
            lifecycleOperation.unlock();
        }
    }

    private static void discardSerialized(Plugin plugin) {
        if (plugin == null) return;
        InvocationState state;
        boolean needsDisable;
        synchronized (lifecycle) {
            if (!isPublishedLocked(plugin)) return;
            state = invocations.get(plugin);
            if (state != null && (state.loading || state.enabling)) {
                state.discardRequested = true;
                state.disableRequested = true;
                return;
            }
            if (state != null && state.disabling) {
                state.discardRequested = true;
                return;
            }
            needsDisable = state != null && !state.disabling && !state.disableComplete;
        }
        if (state == null) return;

        if (needsDisable) disableSerialized(plugin);

        final org.bukkit.plugin.java.PluginClassLoader loader;
        synchronized (lifecycle) {
            removePublishedLocked(plugin);
            loader = loadersByPlugin.get(plugin);
            if (loader != null) {
                pluginLoaders.remove(pluginKey(plugin.getName()), loader);
            }
        }
        awaitInvocations(state, () -> finishDiscard(plugin, state, loader));
    }

    private static void finishDiscard(Plugin plugin, InvocationState state,
            org.bukkit.plugin.java.PluginClassLoader loader) {
        synchronized (lifecycle) {
            invocations.remove(plugin, state);
            if (loader != null) loadersByPlugin.remove(plugin, loader);
            lifecycle.notifyAll();
        }
        if (loader != null) {
            try {
                loader.close();
            } catch (java.io.IOException error) {
                System.out.println("[host] " + plugin.getName()
                    + " classloader failed to close: " + error);
            }
        }
    }

    private static void awaitInvocations(InvocationState state, Runnable onDrained) {
        boolean interrupted = false;
        int operationHolds = 0;
        boolean operationReleased = false;
        try {
            synchronized (lifecycle) {
                if (state.active != 0 && isInsidePluginCallback()) {
                    appendOnDrained(state, onDrained);
                    return;
                }
                operationHolds = lifecycleOperation.getHoldCount();
                while (state.active != 0) {
                    if (!operationReleased) {
                        for (int hold = 0; hold < operationHolds; hold++) {
                            lifecycleOperation.unlock();
                        }
                        operationReleased = true;
                    }
                    try {
                        lifecycle.wait();
                    } catch (InterruptedException ignored) {
                        interrupted = true;
                    }
                }
            }
            // Complete the state transition before reacquiring the operation
            // lock. A callback that acquired it while draining may itself be
            // waiting for this disable to become complete.
            onDrained.run();
        } finally {
            if (operationReleased) {
                for (int hold = 0; hold < operationHolds; hold++) {
                    lifecycleOperation.lock();
                }
            }
        }
        if (interrupted) {
            Thread.currentThread().interrupt();
        }
    }

    private static boolean isInsidePluginCallback() {
        return !threadInvocations.get().isEmpty() || lifecycleCallbackDepth.get() != 0;
    }

    /** Invokes foreign plugin code without making lifecycle helper threads wait on their caller. */
    private static void invokeLifecycleCallback(Runnable callback) {
        int operationHolds = lifecycleOperation.getHoldCount();
        for (int hold = 0; hold < operationHolds; hold++) {
            lifecycleOperation.unlock();
        }
        int depth = lifecycleCallbackDepth.get();
        lifecycleCallbackDepth.set(depth + 1);
        try {
            callback.run();
        } finally {
            if (depth == 0) lifecycleCallbackDepth.remove();
            else lifecycleCallbackDepth.set(depth);
            for (int hold = 0; hold < operationHolds; hold++) {
                lifecycleOperation.lock();
            }
        }
    }

    private static void appendOnDrained(InvocationState state, Runnable action) {
        Runnable previous = state.onDrained;
        state.onDrained = previous == null ? action : () -> {
            previous.run();
            action.run();
        };
    }

    private static final class InvocationState {
        boolean loading;
        boolean enabling;
        boolean disableRequested;
        boolean discardRequested;
        boolean accepting;
        boolean disabling;
        boolean disableComplete;
        int active;
        Runnable onDrained;
    }

    static final class Invocation implements AutoCloseable {
        private InvocationState state;

        private Invocation(InvocationState state) {
            this.state = state;
        }

        @Override public void close() {
            Runnable onDrained = null;
            synchronized (lifecycle) {
                if (state == null) return;
                Map<InvocationState, Integer> current = threadInvocations.get();
                int depth = current.getOrDefault(state, 0);
                if (depth <= 1) current.remove(state);
                else current.put(state, depth - 1);
                state.active--;
                if (state.active == 0) {
                    onDrained = state.onDrained;
                    state.onDrained = null;
                }
                state = null;
                lifecycle.notifyAll();
            }
            if (onDrained != null) onDrained.run();
        }
    }

    /** The plugin with this name, or null. Case-insensitive, as Bukkit is. */
    public static Plugin byName(String name) {
        if (name == null) {
            return null;
        }
        synchronized (lifecycle) {
            return findByNameLocked(name);
        }
    }

    private static Plugin findByNameLocked(String name) {
        for (Plugin plugin : loaded) {
            if (plugin.getName().equalsIgnoreCase(name)) return plugin;
        }
        return null;
    }

    private static boolean isPublishedLocked(Plugin candidate) {
        for (Plugin plugin : loaded) {
            if (plugin == candidate) return true;
        }
        return false;
    }

    private static void removePublishedLocked(Plugin candidate) {
        loaded.removeIf(plugin -> plugin == candidate);
    }

    private static String pluginKey(String name) {
        return name.toLowerCase(java.util.Locale.ROOT);
    }

    /** Everything loaded, in dependency/load order. */
    public static Plugin[] all() {
        synchronized (lifecycle) {
            return loaded.toArray(new Plugin[0]);
        }
    }

    /** Disables and discards everything, newest first, for JVM shutdown. */
    public static void disableAll() {
        lifecycleOperation.lock();
        try {
            List<Plugin> snapshot;
            synchronized (lifecycle) {
                snapshot = new ArrayList<>(loaded);
            }
            for (int index = snapshot.size() - 1; index >= 0; index--) {
                discardSerialized(snapshot.get(index));
            }
        } finally {
            lifecycleOperation.unlock();
        }
    }
}
