package foton;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.net.URLConnection;
import java.nio.file.AtomicMoveNotSupportedException;
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
import java.util.concurrent.locks.ReentrantLock;
import java.util.TreeMap;
import java.util.jar.JarFile;
import java.util.regex.Pattern;
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
    private static final Map<String, String> selectedProviders = new HashMap<>();
    private static final Map<Plugin, org.bukkit.plugin.java.PluginClassLoader> loadersByPlugin =
        new java.util.IdentityHashMap<>();
    private static final Map<Plugin, InvocationState> invocations =
        new java.util.IdentityHashMap<>();
    private static final ThreadLocal<Map<InvocationState, Integer>> threadInvocations =
        ThreadLocal.withInitial(java.util.IdentityHashMap::new);
    private static final ThreadLocal<Integer> lifecycleCallbackDepth =
        ThreadLocal.withInitial(() -> 0);
    private static final Object lifecycle = new Object();
    private static boolean itemBridgeBound;
    private static boolean terminalRequested;
    private static boolean itemBridgeClosed;
    /** Serializes lifecycle callbacks without holding the state monitor.
     *
     * <p>The lock is reentrant because Bukkit permits a plugin callback to
     * disable that same plugin. State transitions still use {@link #lifecycle}
     * so an in-flight event can drain without needing this operation lock. */
    private static final ReentrantLock lifecycleOperation = new ReentrantLock(true);
    /** Plugins Foton ships because the upstream one cannot run here, keyed by lower-case name. */
    private static final Set<String> bundledNames = new HashSet<>();
    /** Where plugins keep their data, bundled ones included; the update folder lives in it. */
    private static volatile File pluginsDirectory = new File("plugins");
    private static final Map<String, org.bukkit.plugin.java.PluginClassLoader> pluginLoaders =
        new java.util.concurrent.ConcurrentHashMap<>();
    private static final Map<String, PluginState> states = new HashMap<>();
    private static final Map<Plugin, PluginState> statesByPlugin =
        new java.util.IdentityHashMap<>();
    private static final Comparator<DiscoveredPlugin> PLUGIN_ORDER =
        Comparator.comparing((DiscoveredPlugin plugin) -> plugin.descriptor.getName(),
            String.CASE_INSENSITIVE_ORDER)
            .thenComparing(plugin -> plugin.descriptor.getName())
            .thenComparing(plugin -> plugin.jar.getName());
    private static final Pattern MAVEN_SEGMENT = Pattern.compile("[A-Za-z0-9_.-]+");

    private enum Status { DISCOVERED, LOADING, LOADED, ENABLING, ENABLED, FAILED, DISABLED }

    private static final class PluginState {
        final PluginDescriptionFile descriptor;
        org.bukkit.plugin.java.PluginClassLoader loader;
        PaperBootstrapRunner bootstrap;
        JavaPlugin plugin;
        Status status = Status.DISCOVERED;
        boolean enableAttempted;
        // Serializes this plugin's final enable commit with cleanup.
        final Object lifecycleLock = new Object();
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

    private PluginHost() {}

    /** Marks a real host shutdown; ordinary plugin disable/re-enable does not call this. */
    public static void markStopping() {
        org.bukkit.Bukkit.markStopping();
        synchronized (lifecycle) { terminalRequested = true; }
        closeItemBridgeIfDrained();
    }

    /** Called only after the embedded host registers its real native methods. */
    public static void bindItemBridge() {
        synchronized (lifecycle) { itemBridgeBound = true; }
        closeItemBridgeIfDrained();
    }

    /** Internal availability signal; registry readiness is checked by native calls. */
    public static boolean itemBridgeBound() {
        synchronized (lifecycle) { return itemBridgeBound; }
    }

    private static void closeItemBridgeIfDrained() {
        boolean close;
        synchronized (lifecycle) {
            close = itemBridgeBound && terminalRequested && !itemBridgeClosed && invocations.isEmpty();
            if (close) itemBridgeClosed = true;
        }
        if (close) Native.closeItemSnapshots();
    }

    /** bukkit.yml's default `settings.update-folder`. */
    public static final String UPDATE_FOLDER = "update";

    /** Where a plugin stages a new copy of its own jar for the next start. */
    public static File updateFolder() {
        return new File(pluginsDirectory, UPDATE_FOLDER);
    }

    static Object lifecycleLock() {
        return lifecycle;
    }

    static void requireEnabled(Plugin plugin, String operation) {
        if (plugin == null) {
            throw new IllegalArgumentException("plugin cannot be null");
        }
        InvocationState registration = invocations.get(plugin);
        if (!plugin.isEnabled() && (registration == null || !registration.loading
                || registration.disabling || registration.discardRequested)) {
            throw new org.bukkit.plugin.IllegalPluginAccessException(
                "Plugin attempted to " + operation + " while disabled: " + plugin.getName());
        }
    }

    static Invocation beginInvocation(Plugin plugin) {
        synchronized (lifecycle) {
            InvocationState state = invocations.get(plugin);
            if (state == null || !state.accepting || !plugin.isEnabled()) {
                return null;
            }
            return acquireInvocation(state);
        }
    }

    /** An open managed task generation also runs during onLoad and before enableAll. */
    static Invocation beginTaskInvocation(FotonScheduler.PluginGeneration generation) {
        synchronized (lifecycle) {
            Plugin plugin = generation.plugin;
            InvocationState state = invocations.get(plugin);
            if (state == null || !isPublishedLocked(plugin) || !generation.accepting()
                    || state.disabling || state.disableComplete
                    || state.disableRequested || state.discardRequested) return null;
            return acquireInvocation(state);
        }
    }

    /** Both callers hold lifecycle, so retirement and admission share one boundary. */
    private static Invocation acquireInvocation(InvocationState state) {
        state.active++;
        threadInvocations.get().merge(state, 1, Integer::sum);
        return new Invocation(state);
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
        initializeHostJdbcProviders();
        ensureServer();
        pluginsDirectory = new File(directory).getAbsoluteFile();
        applyUpdates(pluginsDirectory);
        PluginDiscovery discovery = discoverJars(pluginsDirectory);
        Map<File, PreparedBootstrap> bootstraps = prepareBootstraps(discovery.bootstrapCandidates());
        List<DiscoveredPlugin> ordered = new ArrayList<>();
        for (File jar : discovery.serverOrder()) {
            try { ordered.add(new DiscoveredPlugin(jar, readDescriptor(jar))); }
            catch (Exception error) { System.out.println("[host] " + jar + " descriptor failed: " + error); }
        }
        Map<DiscoveredPlugin, PluginState> prepared = new java.util.IdentityHashMap<>();
        for (DiscoveredPlugin plugin : ordered) {
            try {
                PreparedBootstrap bootstrap = bootstraps.remove(plugin.jar);
                if (plugin.descriptor instanceof PaperPluginDescriptor paper
                        && paper.bootstrapper() != null && bootstrap == null) continue;
                PluginState state = prepare(plugin, bootstrap);
                if (state != null) prepared.put(plugin, state);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        for (PreparedBootstrap unused : bootstraps.values()) closeUnpublished(unused.loader());
        for (DiscoveredPlugin plugin : ordered) {
            PluginState state = prepared.get(plugin);
            if (state == null) continue;
            try {
                construct(plugin, state);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        for (DiscoveredPlugin discovered : ordered) {
            PluginState state = prepared.get(discovered);
            if (state == null || state.plugin == null) continue;
            JavaPlugin plugin = state.plugin;
            String unavailable = unavailablePriorDependency(discovered.descriptor, false);
            if (unavailable != null) {
                System.out.println("[host] " + plugin.getName()
                    + ": required dependency " + unavailable + " did not load");
                synchronized (lifecycle) { invocations.get(plugin).loading = false; }
                rollback(state);
                continue;
            }
            try {
                boolean discardRequested;
                boolean disableRequested;
                try {
                    invokeLifecycleCallback(plugin::onLoad);
                } finally {
                    synchronized (lifecycle) {
                        InvocationState invocation = invocations.get(plugin);
                        discardRequested = invocation != null && invocation.discardRequested;
                        disableRequested = invocation != null && invocation.disableRequested;
                        if (invocation != null) {
                            invocation.loading = false;
                            lifecycle.notifyAll();
                        }
                    }
                    if (discardRequested) discardSerialized(plugin);
                    else if (disableRequested) disableSerialized(plugin);
                }
                if (byName(plugin.getName()) == plugin) state.status = Status.LOADED;
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                rollback(state);
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

    private record PreparedBootstrap(org.bukkit.plugin.java.PluginClassLoader loader,
                                     PaperBootstrapRunner runner, DependencyClassLoader dependencies) {}

    private record BootstrapCandidate(org.bukkit.plugin.java.PluginClassLoader loader,
                                      DependencyClassLoader dependencies,
                                      File jar, PaperPluginDescriptor descriptor) {}

    private static Map<File, PreparedBootstrap> prepareBootstraps(List<File> ordered) {
        Map<File, PreparedBootstrap> prepared = new java.util.LinkedHashMap<>();
        Map<String, File> candidates = new HashMap<>();
        List<PluginDescriptionFile> descriptors = new ArrayList<>();
        for (File jar : ordered) {
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                candidates.put(pluginKey(descriptor.getName()), jar);
                descriptors.add(descriptor);
            } catch (Exception error) {
                System.out.println("[host] " + jar.getName() + " bootstrap descriptor failed: " + error);
            }
        }
        PluginDependencyGraph graph = new PluginDependencyGraph(descriptors);
        List<String> bootstrapOrder = graph.order(PaperPluginDescriptor.Phase.BOOTSTRAP);
        Set<String> failed = new HashSet<>();
        Map<String, org.bukkit.plugin.java.PluginClassLoader> bootstrapLoaders =
            new java.util.concurrent.ConcurrentHashMap<>();
        Map<String, BootstrapCandidate> ready = new java.util.LinkedHashMap<>();
        for (String key : bootstrapOrder) {
            File jar = candidates.get(key);
            org.bukkit.plugin.java.PluginClassLoader loader = null;
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                if (!(descriptor instanceof PaperPluginDescriptor paper) || paper.bootstrapper() == null) continue;
                if (byName(descriptor.getName()) != null) continue;
                DependencyClassLoader dependencies = new DependencyClassLoader(
                    bootstrapDependencies(paper), bootstrapLoaders);
                loader = new org.bukkit.plugin.java.PluginClassLoader(pluginUrls(jar, descriptor), dependencies);
                loader.describe(org.bukkit.Bukkit.getServer(), descriptor,
                    new File(jar.getParentFile(), descriptor.getName()));
                bootstrapLoaders.put(key, loader);
                ready.put(key, new BootstrapCandidate(loader, dependencies, jar, paper));
            } catch (Throwable error) {
                failed.add(key);
                System.out.println("[host] " + jar.getName() + " bootstrap classloader failed: " + error);
                if (loader != null) closeUnpublished(loader);
            }
        }
        propagateBootstrapFailures(ready, graph.providers(), failed);
        for (String key : bootstrapOrder) {
            BootstrapCandidate candidate = ready.get(key);
            if (candidate == null || failed.contains(key)) continue;
            try {
                PaperBootstrapRunner runner = new PaperBootstrapRunner(candidate.jar(),
                    candidate.descriptor(), candidate.loader());
                runner.bootstrap();
                prepared.put(candidate.jar(), new PreparedBootstrap(candidate.loader(), runner,
                    candidate.dependencies()));
            } catch (Throwable error) {
                System.out.println("[host] " + candidate.jar().getName() + " bootstrap failed: " + error);
                failed.add(key);
                propagateBootstrapFailures(ready, graph.providers(), failed);
            }
        }
        for (String key : bootstrapOrder) {
            BootstrapCandidate candidate = ready.get(key);
            if (candidate == null || failed.contains(key)) continue;
            PreparedBootstrap bootstrap = prepared.get(candidate.jar());
            if (bootstrap == null) continue;
            try {
                bootstrap.runner().commands();
            } catch (Throwable error) {
                System.out.println("[host] " + candidate.jar().getName() + " bootstrap commands failed: " + error);
                failed.add(key);
                propagateBootstrapFailures(ready, graph.providers(), failed);
            }
        }
        // Ordering recovery never turns actual required-provider failure into
        // success. Retire earlier cycle consumers too, before SERVER construction.
        for (String key : failed) {
            BootstrapCandidate candidate = ready.get(key);
            if (candidate == null) continue;
            prepared.remove(candidate.jar());
            closeUnpublished(candidate.loader());
        }
        // Bootstrap visibility ends here. A surviving plugin loader must not
        // retain every sibling (including failed, closed loaders) for its lifetime.
        bootstrapLoaders.clear();
        return prepared;
    }

    private static void propagateBootstrapFailures(Map<String, BootstrapCandidate> ready,
            Map<String, String> providers, Set<String> failed) {
        boolean changed;
        do {
            changed = false;
            for (var entry : ready.entrySet()) {
                if (failed.contains(entry.getKey())) continue;
                for (String dependency : PluginDependencyGraph.required(entry.getValue().descriptor(),
                        PaperPluginDescriptor.Phase.BOOTSTRAP)) {
                    String provider = providers.getOrDefault(pluginKey(dependency), pluginKey(dependency));
                    if (!failed.contains(provider)) continue;
                    System.out.println("[host] " + entry.getValue().descriptor().getName()
                        + " rejected: required BOOTSTRAP provider failed: " + dependency);
                    changed |= failed.add(entry.getKey());
                    break;
                }
            }
        } while (changed);
    }

    private static List<String> bootstrapDependencies(PaperPluginDescriptor descriptor) {
        // Pinned Paper 26.2 build 129 exposes a declared bootstrap provider's
        // classes even when that edge declares join-classpath: false. The same
        // flag remains honored independently for the SERVER phase.
        return descriptor.paperDependencies().stream()
            .filter(dependency -> dependency.phase() == PaperPluginDescriptor.Phase.BOOTSTRAP)
            .map(PaperPluginDescriptor.Dependency::name).toList();
    }

    private static void closeUnpublished(org.bukkit.plugin.java.PluginClassLoader loader) {
        if (loader.getPlugin() != null) {
            JavaPlugin plugin = loader.getPlugin();
            plugin.setEnabled(false);
            cleanupPublished(plugin);
            release(plugin, "permissions", () -> PermissionRegistry.removeOwnedBy(plugin));
        }
        try { loader.close(); }
        catch (IOException error) { System.out.println("[host] bootstrap classloader failed to close: " + error); }
    }

    private static List<String> requiredDependencies(Plugin plugin) {
        return requiredDependencies(plugin.getDescription());
    }

    /** Dependencies without which the plugin must not load: Bukkit's
     * {@code depend}, or the {@code required} entries of paper-plugin.yml. */
    private static List<String> requiredDependencies(PluginDescriptionFile descriptor) {
        return PluginDependencyGraph.required(descriptor, PaperPluginDescriptor.Phase.SERVER);
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
        Set<Plugin> enabledNow = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<>());
        for (Plugin plugin : List.of(all())) {
            if (plugin.isEnabled() || !(plugin instanceof JavaPlugin java)) continue;
            PluginState state = stateOf(plugin);
            synchronized (lifecycle) {
                InvocationState invocation = invocations.get(plugin);
                if (invocation == null || invocation.loading || invocation.enabling || invocation.disabling) continue;
            }
            String unavailable = unavailablePriorDependency(plugin.getDescription(), true);
            if (unavailable != null) {
                System.out.println("[host] " + plugin.getName()
                    + ": required dependency " + unavailable + " did not enable");
                if (state != null) state.status = Status.FAILED;
                if (byName(unavailable) == null) cleanup(plugin);
                else disable(plugin);
                continue;
            }
            try {
                enable(java);
                if (plugin.isEnabled()) enabledNow.add(plugin);
            } catch (Throwable error) {
                System.out.println("[host] " + plugin.getName() + " failed: " + error);
                error.printStackTrace(System.out);
                if (state != null) state.status = Status.FAILED;
                synchronized (lifecycle) {
                    InvocationState invocation = invocations.get(plugin);
                    if (invocation != null) invocation.deferDrain = true;
                }
                disable(plugin);
            }
        }
        propagateRequiredFailures(true);
        enabledNow.removeIf(plugin -> !plugin.isEnabled() || byName(plugin.getName()) != plugin);
        return enabledNow.size();
    }

    private static void ensureServer() {
        if (org.bukkit.Bukkit.getServer() == null) {
            org.bukkit.Bukkit.setServer(new FotonServer());
        }
    }

    /** Initializes JDBC services owned by the stable host class loader.
     *
     * DriverManager otherwise discovers providers through the calling thread's
     * context loader. A server embedding the JVM cannot assume that loader can
     * see the host class path. Deliberately naming the host loader also keeps
     * plugin-private providers out of DriverManager's process-global registry.
     */
    private static void initializeHostJdbcProviders() {
        HostJdbcProviders.initialize();
    }

    private static final class HostJdbcProviders {
        static {
            for (java.sql.Driver ignored : java.util.ServiceLoader.load(
                    java.sql.Driver.class, PluginHost.class.getClassLoader())) {
                // Instantiating a host provider runs its standard JDBC
                // registration path. The provider remains host-owned.
            }
        }

        private static void initialize() {}
    }

    /** SimplePluginManager's update step: a jar in the update folder with the
     * same file name as a plugin jar replaces it before anything is loaded,
     * and is removed once copied. A self-updating plugin stages its download
     * there because the running jar cannot be overwritten safely. */
    private static void applyUpdates(File dir) {
        File updates = new File(dir, UPDATE_FOLDER);
        File[] jars = dir.listFiles((d, name) -> name.endsWith(".jar"));
        if (jars == null || !updates.isDirectory()) return;
        for (File jar : jars) {
            File staged = new File(updates, jar.getName());
            if (!staged.isFile()) continue;
            try {
                Files.copy(staged.toPath(), jar.toPath(), StandardCopyOption.REPLACE_EXISTING);
                Files.delete(staged.toPath());
                System.out.println("[host] updated " + jar.getName() + " from " + UPDATE_FOLDER + "/");
            } catch (java.io.IOException error) {
                System.out.println("[host] could not apply update for " + jar.getName() + ": " + error);
            }
        }
    }

    private record PluginDiscovery(List<File> serverOrder, List<File> bootstrapCandidates) {}

    private static PluginDiscovery discoverJars(File dir) {
        File[] found = dir.listFiles((d, name) -> name.endsWith(".jar"));
        List<File> bundled = bundledJars();
        List<File> jars = new ArrayList<>(bundled);
        if (found != null) {
            java.util.Arrays.sort(found, Comparator.comparing(File::getName));
            jars.addAll(List.of(found));
        }
        List<DiscoveredPlugin> discovered = new ArrayList<>();
        for (File jar : jars) {
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                requireSupportedLoading(descriptor);
                String identity = key(descriptor.getName());
                if (!bundled.contains(jar) && bundledNames.contains(identity)) {
                    System.out.println("[host] " + descriptor.getName() + " is provided by Foton; skipping " + jar.getName());
                    continue;
                }
                if (bundled.contains(jar)) bundledNames.add(identity);
                discovered.add(new DiscoveredPlugin(jar, descriptor));
            } catch (Throwable error) {
                System.out.println("[host] " + jar.getName() + " failed: " + error);
                error.printStackTrace(System.out);
            }
        }
        Set<DiscoveredPlugin> rejected = new HashSet<>();
        rejectDuplicateNames(discovered, rejected);
        rejectLoadedIdentityConflicts(discovered, rejected);
        Map<String, File> jarsByName = new HashMap<>();
        List<PluginDescriptionFile> descriptors = new ArrayList<>();
        for (Plugin live : all()) descriptors.add(live.getDescription());
        List<File> bootstrapCandidates = new ArrayList<>();
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            descriptors.add(plugin.descriptor);
            jarsByName.put(plugin.key, plugin.jar);
            bootstrapCandidates.add(plugin.jar);
        }
        PluginDependencyGraph graph = new PluginDependencyGraph(descriptors);
        synchronized (lifecycle) {
            if (loaded.isEmpty()) selectedProviders.clear();
            // Live selections are stable across incremental discovery.
            graph.providers().forEach(selectedProviders::putIfAbsent);
        }
        List<File> ordered = new ArrayList<>();
        for (String identity : graph.order(PaperPluginDescriptor.Phase.SERVER)) {
            File jar = jarsByName.get(identity);
            if (jar != null) ordered.add(jar);
        }
        return new PluginDiscovery(ordered, bootstrapCandidates);
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

    private static void rejectLoadedIdentityConflicts(
            List<DiscoveredPlugin> discovered, Set<DiscoveredPlugin> rejected) {
        for (DiscoveredPlugin plugin : discovered) {
            if (rejected.contains(plugin)) continue;
            IdentityConflict conflict = loadedIdentityConflict(plugin.descriptor);
            if (conflict == null) continue;
            System.out.println("[host] " + plugin.descriptor.getName() + " (" + plugin.jar.getName() + "): identity "
                + conflict.identity + " is already provided by " + conflict.owner.getName());
            rejected.add(plugin);
        }
    }

    /** Jars Foton ships beside its API, from {@code -Dfoton.bundled-plugins}. */
    private static List<File> bundledJars() {
        String directory = System.getProperty("foton.bundled-plugins");
        if (directory == null || directory.isEmpty()) return List.of();
        File[] jars = new File(directory).listFiles((d, name) -> name.endsWith(".jar"));
        if (jars == null) return List.of();
        List<File> sorted = new ArrayList<>(List.of(jars));
        sorted.sort(java.util.Comparator.comparing(File::getName));
        return sorted;
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

    private static void requireSupportedLoading(PluginDescriptionFile descriptor)
            throws InvalidDescriptionException {
        if (descriptor instanceof PaperPluginDescriptor paper) {
            paper.validateTarget(org.bukkit.Bukkit.getMinecraftVersion());
            if (paper.hasOpenClassloader()) {
                throw new UnsupportedOperationException(descriptor.getName()
                    + ": open Paper classloader visibility is not implemented");
            }
        }
    }

    private static PluginState prepare(DiscoveredPlugin discovered, PreparedBootstrap prepared) throws Throwable {
        File jar = discovered.jar;
        PluginDescriptionFile descriptor = discovered.descriptor;
        requireSupportedLoading(descriptor);
        IdentityConflict conflict = loadedIdentityConflict(descriptor);
        if (conflict != null) {
            System.out.println("[host] " + descriptor.getName() + ": identity "
                + conflict.identity + " is already provided by " + conflict.owner.getName()
                + "; skipping " + jar.getName());
            if (prepared != null) closeUnpublished(prepared.loader());
            return null;
        }
        PluginState state = new PluginState(descriptor);
        state.bootstrap = prepared == null ? null : prepared.runner();
        registerState(state);
        try {
            org.bukkit.plugin.java.PluginClassLoader loader =
                prepared == null ? new org.bukkit.plugin.java.PluginClassLoader(
                    pluginUrls(jar, descriptor), dependencyParent(descriptor)) : prepared.loader();
            state.loader = loader;
            if (prepared != null) prepared.dependencies().useServerDependencies(
                PluginDependencyGraph.visible(descriptor, PaperPluginDescriptor.Phase.SERVER));
            File dataFolder = new File(bundledNames.contains(key(descriptor.getName()))
                ? pluginsDirectory : jar.getParentFile(), descriptor.getName());
            // Before the constructor, not after: a plugin may call getName()
            // or getLogger() from it, and several do.
            loader.describe(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            registerLoader(descriptor, loader);
            return state;
        } catch (Throwable error) {
            rollback(state);
            throw error;
        }
    }

    private static void construct(
            DiscoveredPlugin discovered, PluginState state) throws Throwable {
        File jar = discovered.jar;
        PluginDescriptionFile descriptor = discovered.descriptor;
        org.bukkit.plugin.java.PluginClassLoader loader = state.loader;
        try {
            File dataFolder = new File(bundledNames.contains(key(descriptor.getName()))
                ? pluginsDirectory : jar.getParentFile(), descriptor.getName());
            JavaPlugin candidate = state.bootstrap == null
                ? constructMain(descriptor, loader) : state.bootstrap.createPlugin();
            // Foton's existing explicit-null fallback uses the same owning loader.
            if (candidate == null) candidate = constructMain(descriptor, loader);
            if (state.bootstrap != null
                    && (candidate.getClass().getClassLoader() != loader || loader.getPlugin() != candidate)) {
                throw new IllegalStateException("createPlugin must return the JavaPlugin constructed by its own loader");
            }
            state.plugin = candidate;
            synchronized (lifecycle) { statesByPlugin.put(candidate, state); }

            loader.setPlugin(candidate);
            candidate.init(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);

            synchronized (lifecycle) {
                if (terminalRequested) throw new IllegalStateException("host is stopping");
                loaded.add(candidate);
                InvocationState invocation = new InvocationState();
                invocation.loading = true;
                invocations.put(candidate, invocation);
                loadersByPlugin.put(candidate, loader);
                state.status = Status.LOADING;
                FotonScheduler.activatePlugin(candidate);
            }
            PermissionRegistry.register(candidate, descriptor.getPermissions());
        } catch (Throwable error) {
            if (state.plugin == null && loader != null) {
                state.plugin = loader.getPlugin();
                if (state.plugin != null) statesByPlugin.put(state.plugin, state);
            }
            rollback(state);
            throw error;
        }
    }

    private static JavaPlugin constructMain(
            PluginDescriptionFile descriptor,
            org.bukkit.plugin.java.PluginClassLoader loader) throws ReflectiveOperationException {
        Class<?> type = Class.forName(descriptor.getMain(), true, loader);
        Object instance = type.getDeclaredConstructor().newInstance();
        if (instance instanceof JavaPlugin candidate) return candidate;
        throw new IllegalArgumentException("main class " + descriptor.getMain()
            + " is not a JavaPlugin");
    }

    private static void rollback(PluginState state) {
        state.status = Status.FAILED;
        if (state.plugin != null) {
            JavaPlugin plugin = state.plugin;
            boolean published;
            synchronized (lifecycle) { published = invocations.containsKey(plugin); }
            if (published) {
                synchronized (lifecycle) { invocations.get(plugin).loading = false; }
                cleanup(plugin);
                return;
            }
            // Construction failed before publication: it can still have registered
            // commands/lifecycle handlers through the classloader's early identity.
            plugin.setEnabled(false);
            cleanupPublished(plugin);
            release(plugin, "permissions", () -> PermissionRegistry.removeOwnedBy(plugin));
            synchronized (lifecycle) { statesByPlugin.remove(plugin); }
            state.plugin = null;
        }
        state.cleaned.set(true);
        if (state.loader != null) {
            org.bukkit.plugin.java.PluginClassLoader loader = state.loader;
            pluginLoaders.entrySet().removeIf(entry -> entry.getValue() == loader);
            close(loader, state.descriptor.getName());
        }
        state.loader = null;
    }

    private static String unavailablePriorDependency(
            PluginDescriptionFile descriptor, boolean enabling) {
        for (String dependency : requiredDependencies(descriptor)) {
            Plugin provider = byName(dependency);
            PluginState state = provider == null ? null : stateOf(provider);
            if (state == null || state.status == Status.FAILED
                    || enabling && state.enableAttempted && !provider.isEnabled())
                return dependency;
        }
        return null;
    }

    private static void propagateRequiredFailures(boolean enabling) {
        boolean changed;
        do {
            changed = false;
            for (Plugin plugin : List.of(all())) {
                PluginState state = stateOf(plugin);
                if (state == null || (enabling && !plugin.isEnabled())) continue;
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
        List<String> identities = selectedIdentityKeys(state.descriptor);
        for (String identity : identities) {
            PluginState existing = states.get(identity);
            if (existing != null && existing.plugin != null && loaded.contains(existing.plugin)) {
                throw new IllegalStateException("plugin identity is already registered: " + identity);
            }
        }
        for (String identity : identities) states.put(identity, state);
    }

    private static void registerLoader(
            PluginDescriptionFile descriptor,
            org.bukkit.plugin.java.PluginClassLoader loader) {
        List<String> identities = selectedIdentityKeys(descriptor);
        for (String identity : identities) {
            if (pluginLoaders.containsKey(identity)) {
                throw new IllegalStateException("plugin identity is already registered: " + identity);
            }
        }
        for (String identity : identities) pluginLoaders.put(identity, loader);
    }

    private static List<String> selectedIdentityKeys(PluginDescriptionFile descriptor) {
        String owner = key(descriptor.getName());
        return identityKeys(descriptor).stream().filter(identity ->
            owner.equals(selectedProviders.getOrDefault(identity, owner))).toList();
    }

    private static void addDependencyIdentity(List<String> out, String name) {
        String identity = key(name);
        if (!out.contains(identity)) out.add(identity);
    }

    private static List<String> identityKeys(PluginDescriptionFile descriptor) {
        List<String> identities = new ArrayList<>();
        addDependencyIdentity(identities, descriptor.getName());
        for (String alias : descriptor.getProvides()) addDependencyIdentity(identities, alias);
        return identities;
    }

    private static IdentityConflict loadedIdentityConflict(PluginDescriptionFile descriptor) {
        for (String identity : identityKeys(descriptor)) {
            Plugin owner = byName(identity);
            if (owner != null) return new IdentityConflict(identity, owner);
        }
        return null;
    }

    private static final class IdentityConflict {
        final String identity;
        final Plugin owner;

        IdentityConflict(String identity, Plugin owner) {
            this.identity = identity;
            this.owner = owner;
        }
    }

    private static void addKotlinSerializationCompanion(List<URL> urls, Path cache,
            MavenCoordinate coordinate) throws Exception {
        if (coordinate.group.equals("org.jetbrains.kotlinx")
                && coordinate.artifact.equals("kotlinx-serialization-json-jvm")) {
            URL companion = resolveLibrary(cache, new MavenCoordinate(coordinate.group,
                "kotlinx-serialization-core-jvm", coordinate.version)).toUri().toURL();
            if (!urls.contains(companion)) urls.add(companion);
        }
    }

    private static URL[] pluginUrls(File jar, PluginDescriptionFile descriptor) throws Exception {
        List<URL> urls = new ArrayList<>();
        urls.add(jar.toURI().toURL());
        Path cache = jar.toPath().toAbsolutePath().getParent().resolve(".foton-libraries");
        List<io.papermc.paper.plugin.loader.library.ClassPathLibrary> libraries = new ArrayList<>();
        io.papermc.paper.plugin.loader.library.ClassPathLibrary legacyLibraries = null;
        if (!(descriptor instanceof PaperPluginDescriptor) && !descriptor.isPaperSkipLibraries()
                && !descriptor.getLibraries().isEmpty()) {
            var resolver = new io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver();
            resolver.addRepository(new org.eclipse.aether.repository.RemoteRepository.Builder("central", "default",
                io.papermc.paper.plugin.loader.library.impl.MavenLibraryResolver.MAVEN_CENTRAL_DEFAULT_MIRROR).build());
            for (String coordinate : descriptor.getLibraries()) {
                // Preserve the pre-existing validated flat-cache contract. New
                // uncached declarations use the incoming transitive resolver.
                String[] parts = coordinate.split(":", -1);
                if (parts.length == 3 && java.util.Arrays.stream(parts)
                        .allMatch(part -> !part.isEmpty() && !part.equals(".") && !part.equals("..")
                            && MAVEN_SEGMENT.matcher(part).matches())) {
                    MavenCoordinate cached = MavenCoordinate.parse(coordinate).runtimeVariant();
                    Path existing = cached.cachePath(cache);
                    if (validJar(existing)) {
                        urls.add(existing.toUri().toURL());
                        addKotlinSerializationCompanion(urls, cache, cached);
                        continue;
                    }
                }
                resolver.addDependency(new org.eclipse.aether.graph.Dependency(new org.eclipse.aether.artifact.DefaultArtifact(coordinate), null));
            }
            legacyLibraries = resolver;
        }
        String loaderName = descriptor instanceof PaperPluginDescriptor paper ? paper.loader() : descriptor.getPaperPluginLoader();
        if (loaderName == null) {
            // Foton compatibility adapter predating Paper's custom loader contract.
            // Custom loaders own their JSON exclusively; never inspect it in that path.
            List<String> declarations;
            try (JarFile archive = new JarFile(jar)) { declarations = paperLibraries(archive); }
            for (String declaration : declarations) {
                MavenCoordinate coordinate = MavenCoordinate.parse(declaration).runtimeVariant();
                urls.add(resolveLibrary(cache, coordinate).toUri().toURL());
                addKotlinSerializationCompanion(urls, cache, coordinate);
            }
        }
        if (loaderName != null) {
            var context = new io.papermc.paper.plugin.bootstrap.PluginProviderContext() {
                @Override public io.papermc.paper.plugin.configuration.PluginMeta getConfiguration() { return descriptor; }
                @Override public Path getDataDirectory() { return jar.toPath().toAbsolutePath().getParent().resolve(descriptor.getName()); }
                @Override public net.kyori.adventure.text.logger.slf4j.ComponentLogger getLogger() {
                    return net.kyori.adventure.text.logger.slf4j.ComponentLogger.logger(descriptor.getName());
                }
                @Override public Path getPluginSource() { return jar.toPath(); }
            };
            try (URLClassLoader loader = new URLClassLoader(new URL[] {jar.toURI().toURL()}, PluginHost.class.getClassLoader())) {
                Object instance = Class.forName(loaderName, true, loader).getDeclaredConstructor().newInstance();
                if (!(instance instanceof io.papermc.paper.plugin.loader.PluginLoader pluginLoader)) {
                    throw new IOException("Plugin loader does not implement PluginLoader: " + loaderName);
                }
                pluginLoader.classloader(new io.papermc.paper.plugin.loader.PluginClasspathBuilder() {
                    @Override public io.papermc.paper.plugin.loader.PluginClasspathBuilder addLibrary(io.papermc.paper.plugin.loader.library.ClassPathLibrary library) {
                        libraries.add(java.util.Objects.requireNonNull(library));
                        return this;
                    }
                    @Override public io.papermc.paper.plugin.bootstrap.PluginProviderContext getContext() { return context; }
                });
                if (legacyLibraries != null) libraries.add(legacyLibraries);
                // A custom ClassPathLibrary can reference classes from the loader's JAR.
                urls.addAll(PluginLibraryResolver.registerLibraries(cache, libraries));
            }
        } else {
            if (legacyLibraries != null) libraries.add(legacyLibraries);
            urls.addAll(PluginLibraryResolver.registerLibraries(cache, libraries));
        }
        return urls.toArray(new URL[0]);
    }

    private static List<String> paperLibraries(JarFile archive) throws IOException {
        var entry = archive.getEntry("paper-libraries.json");
        if (entry == null) return List.of();
        Object document;
        try (InputStream stream = archive.getInputStream(entry)) {
            document = new org.yaml.snakeyaml.Yaml().load(stream);
        } catch (RuntimeException error) {
            throw new IOException("invalid paper-libraries.json", error);
        }
        if (!(document instanceof Map<?, ?> root)) {
            throw new IOException("paper-libraries.json must contain an object");
        }
        Object dependencies = root.get("dependencies");
        if (dependencies == null) return List.of();
        if (!(dependencies instanceof Collection<?> values)) {
            throw new IOException("paper-libraries.json dependencies must be a list");
        }
        List<String> declarations = new ArrayList<>();
        for (Object value : values) {
            if (!(value instanceof String declaration)) {
                throw new IOException("paper-libraries.json dependencies must be strings");
            }
            declarations.add(declaration);
        }
        return declarations;
    }

    private static Path resolveLibrary(Path cache, MavenCoordinate coordinate)
            throws IOException {
        Path target = coordinate.cachePath(cache);
        cacheLibrary(coordinate.mavenUrl(), target);
        return target;
    }

    /** Validates and atomically publishes a host-owned dependency cache entry. */
    static void cacheLibrary(URL source, Path target) throws IOException {
        if (validJar(target)) return;
        Files.createDirectories(target.getParent());
        Path temporary = createLibraryTemporary(target);
        try {
            URLConnection connection = source.openConnection();
            connection.setConnectTimeout(10_000);
            connection.setReadTimeout(30_000);
            try (InputStream input = connection.getInputStream()) {
                Files.copy(input, temporary, StandardCopyOption.REPLACE_EXISTING);
            }
            publishLibrary(temporary, target);
        } finally {
            Files.deleteIfExists(temporary);
        }
    }

    private static Path createLibraryTemporary(Path target) throws IOException {
        return Files.createTempFile(
            target.getParent(), target.getFileName() + ".", ".tmp");
    }

    private static boolean validJar(Path path) {
        if (!Files.isRegularFile(path)) return false;
        try {
            validateJar(path);
            return true;
        } catch (IOException | SecurityException invalid) {
            return false;
        }
    }

    private static void validateJar(Path path) throws IOException {
        if (!Files.isRegularFile(path)) throw new IOException("Runtime library is not a regular file: " + path);
        boolean hasFile = false;
        try (JarFile archive = new JarFile(path.toFile(), true)) {
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
        if (!hasFile) throw new IOException("Runtime library contains no files: " + path);
    }

    private static void publishLibrary(Path temporary, Path target) throws IOException {
        validateJar(temporary);
        try {
            Files.move(temporary, target,
                StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException error) {
            if (!validJar(target)) throw error;
        }
    }

    private record MavenCoordinate(String group, String artifact, String version) {
        static MavenCoordinate parse(String declaration) {
            String[] parts = declaration == null
                ? new String[0]
                : declaration.split(":", -1);
            if (parts.length != 3) {
                throw invalid(declaration, "expected group:artifact:version");
            }
            String[] groupSegments = parts[0].split("\\.", -1);
            for (String segment : groupSegments) {
                validateSegment(segment, "group", declaration);
            }
            validateSegment(parts[1], "artifact", declaration);
            validateSegment(parts[2], "version", declaration);
            return new MavenCoordinate(parts[0], parts[1], parts[2]);
        }

        MavenCoordinate runtimeVariant() {
            if (!group.startsWith("org.jetbrains.kotlinx") || artifact.endsWith("-jvm")) {
                return this;
            }
            return new MavenCoordinate(group, artifact + "-jvm", version);
        }

        Path cachePath(Path cache) {
            Path target = cache;
            for (String segment : group.split("\\.")) target = target.resolve(segment);
            return target.resolve(artifact).resolve(version)
                .resolve(artifact + "-" + version + ".jar");
        }

        URL mavenUrl() throws IOException {
            String path = group.replace('.', '/') + "/" + artifact + "/" + version;
            String file = artifact + "-" + version + ".jar";
            try {
                return java.net.URI.create(
                    "https://repo.maven.apache.org/maven2/" + path + "/" + file).toURL();
            } catch (IllegalArgumentException error) {
                throw new IOException("invalid Maven Central URL for " + declaration(), error);
            }
        }

        String declaration() {
            return group + ":" + artifact + ":" + version;
        }

        private static void validateSegment(
                String segment, String kind, String declaration) {
            if (segment.isEmpty() || segment.equals(".") || segment.equals("..")
                    || !MAVEN_SEGMENT.matcher(segment).matches()) {
                throw invalid(declaration, "invalid " + kind + " segment");
            }
        }

        private static IllegalArgumentException invalid(String declaration, String reason) {
            return new IllegalArgumentException(
                "invalid Maven coordinate " + declaration + ": " + reason);
        }
    }

    private static ClassLoader dependencyParent(PluginDescriptionFile descriptor) {
        List<String> visible = PluginDependencyGraph.visible(descriptor, PaperPluginDescriptor.Phase.SERVER);
        return visible.isEmpty() ? PluginHost.class.getClassLoader() : new DependencyClassLoader(visible);
    }

    private static final class DependencyClassLoader extends ClassLoader {
        private static final ThreadLocal<Set<DependencyClassLoader>> searching =
            ThreadLocal.withInitial(HashSet::new);
        private volatile List<String> dependencies;
        private final Map<String, org.bukkit.plugin.java.PluginClassLoader> bootstrapLoaders;
        private volatile boolean bootstrapPhase;

        private DependencyClassLoader(List<String> dependencies) {
            this(dependencies, Map.of(), false);
        }

        private DependencyClassLoader(List<String> dependencies,
                Map<String, org.bukkit.plugin.java.PluginClassLoader> bootstrapLoaders) {
            this(dependencies, bootstrapLoaders, true);
        }

        private DependencyClassLoader(List<String> dependencies,
                Map<String, org.bukkit.plugin.java.PluginClassLoader> bootstrapLoaders,
                boolean bootstrapPhase) {
            super(PluginHost.class.getClassLoader());
            this.dependencies = List.copyOf(dependencies);
            this.bootstrapLoaders = bootstrapLoaders;
            this.bootstrapPhase = bootstrapPhase;
        }

        private void useServerDependencies(List<String> serverDependencies) {
            this.dependencies = List.copyOf(serverDependencies);
            this.bootstrapPhase = false;
        }

        @Override
        protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
            try {
                return super.loadClass(name, resolve);
            } catch (ClassNotFoundException missingFromServer) {
                Set<DependencyClassLoader> active = searching.get();
                if (!active.add(this)) throw missingFromServer;
                try {
                    List<ClassLoader> available = new ArrayList<>();
                    synchronized (lifecycle) {
                        for (String dependency : dependencies) {
                            ClassLoader loader;
                            if (bootstrapPhase) {
                                String alias = selectedProviders.getOrDefault(pluginKey(dependency),
                                    pluginKey(dependency));
                                loader = bootstrapLoaders.get(alias);
                            } else {
                                Plugin plugin = findByNameLocked(dependency);
                                loader = loadersByPlugin.get(plugin);
                                if (loader == null) loader = pluginLoaders.get(pluginKey(dependency));
                            }
                            if (loader != null) available.add(loader);
                        }
                    }
                    // AFTER and OMIT providers may be published after this
                    // loader was constructed. Visibility is not a load-order edge.
                    for (ClassLoader dependency : available) {
                        try {
                            return Class.forName(name, false, dependency);
                        } catch (ClassNotFoundException ignored) {
                            // Try the next declared dependency.
                        }
                    }
                } finally {
                    active.remove(this);
                    if (active.isEmpty()) searching.remove();
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
            FotonScheduler.activatePlugin(plugin);
            PluginState metadata = stateOf(plugin);
            if (metadata != null) { metadata.enableAttempted = true; metadata.status = Status.ENABLING; }
            plugin.setEnabled(true);
        }
        boolean disableRequested;
        boolean discardRequested;
        try {
            Runnable commands = FotonLifecycle.commandDispatcher(plugin);
            invokeLifecycleCallback(commands);
            invokeLifecycleCallback(plugin::onEnable);
            Invocation commandInvocation;
            synchronized (lifecycle) {
                InvocationState state = invocations.get(plugin);
                commandInvocation = state != null && state.accepting && plugin.isEnabled()
                    && isPublishedLocked(plugin) && !state.disableRequested && !state.discardRequested
                    ? acquireInvocation(state) : null;
            }
            if (commandInvocation != null) {
                try (commandInvocation) {
                    invokeLifecycleCallback(commands);
                }
            }
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
                cleanup(plugin);
                return;
            }
        }
        invokeLifecycleCallback(() ->
            EventBridge.dispatch(new org.bukkit.event.server.PluginEnableEvent(plugin)));
        synchronized (lifecycle) {
            if (!plugin.isEnabled() || !isPublishedLocked(plugin)) {
                cleanup(plugin);
                return;
            }
            PluginState metadata = stateOf(plugin);
            if (metadata != null) metadata.status = Status.ENABLED;
        }
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
                FotonScheduler.stopPluginSubmissions(plugin);
                return;
            }
            state.accepting = false;
            state.disabling = true;
            FotonScheduler.stopPluginSubmissions(plugin);
            PluginState metadata = stateOf(plugin);
            if (metadata != null && metadata.status != Status.FAILED) metadata.status = Status.DISABLED;
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
                error.printStackTrace(System.out);
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
        FotonScheduler.beginPluginCleanup(plugin);
        release(plugin, "scheduler tasks", () -> org.bukkit.Bukkit.getScheduler().cancelTasks(plugin));
        release(plugin, "permission attachments", () -> FotonPlayer.removeAttachments(plugin));
        release(plugin, "commands", () -> CommandMap.forget(plugin));
        release(plugin, "services", () -> org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin));
        release(plugin, "incoming channels", () -> org.bukkit.Bukkit.getMessenger().unregisterIncomingPluginChannel(plugin));
        release(plugin, "outgoing channels", () -> org.bukkit.Bukkit.getMessenger().unregisterOutgoingPluginChannel(plugin));
        release(plugin, "listeners", () -> EventBridge.unregister(plugin));
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

    /** Permanently retires this generation, as used by rollback and explicit cleanup. */
    public static void cleanup(Plugin plugin) {
        synchronized (lifecycle) {
            InvocationState invocation = invocations.get(plugin);
            if (invocation != null) invocation.deferDrain = true;
        }
        discard(plugin);
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
            state = invocations.get(plugin);
            if (!isPublishedLocked(plugin) && (state == null || !state.discardRequested)) return;
            if (state != null && (state.loading || state.enabling)) {
                state.discardRequested = true;
                state.disableRequested = true;
                FotonScheduler.stopPluginSubmissions(plugin);
                return;
            }
            if (state != null && state.disabling) {
                state.discardRequested = true;
                if (state.deferDrain) unpublishLocked(plugin);
                return;
            }
            needsDisable = state != null && !state.disabling && !state.disableComplete;
        }
        if (state == null) return;

        if (needsDisable) disableSerialized(plugin);

        final org.bukkit.plugin.java.PluginClassLoader loader;
        synchronized (lifecycle) {
            loader = loadersByPlugin.get(plugin);
            unpublishLocked(plugin);
        }
        awaitInvocations(state, () -> finishDiscard(plugin, state, loader));
    }

    private static void finishDiscard(Plugin plugin, InvocationState state,
            org.bukkit.plugin.java.PluginClassLoader loader) {
        release(plugin, "permissions", () -> PermissionRegistry.removeOwnedBy(plugin));
        synchronized (lifecycle) {
            invocations.remove(plugin, state);
            PluginState metadata = statesByPlugin.remove(plugin);
            if (metadata != null) {
                metadata.cleaned.set(true);
                metadata.plugin = null;
                metadata.loader = null;
                metadata.bootstrap = null;
            }
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
        closeItemBridgeIfDrained();
    }

    private static void awaitInvocations(InvocationState state, Runnable onDrained) {
        boolean interrupted = false;
        int operationHolds = 0;
        boolean operationReleased = false;
        try {
            synchronized (lifecycle) {
                if (state.active != 0 && (state.deferDrain || isInsidePluginCallback())) {
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
        boolean deferDrain;
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

    private static void release(Plugin plugin, String resource, Runnable action) {
        try {
            action.run();
        } catch (Throwable error) {
            System.out.println("[host] " + plugin.getName() + " failed to release "
                + resource + ": " + error);
            error.printStackTrace(System.out);
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
        synchronized (lifecycle) {
            return findByNameLocked(name);
        }
    }

    private static Plugin findByNameLocked(String name) {
        String selected = selectedProviders.get(pluginKey(name));
        return findExactNameLocked(selected == null ? name : selected);
    }

    private static Plugin findExactNameLocked(String name) {
        for (Plugin plugin : loaded) {
            if (plugin.getName().equalsIgnoreCase(name)) return plugin;
        }
        for (Plugin plugin : loaded) {
            if (containsIgnoreCase(plugin.getDescription().getProvides(), name)) {
                return plugin;
            }
        }
        return null;
    }

    /** Bootstrap callbacks ran once before construction; seed each instance enable cycle. */
    static void seedBootstrapCommands(JavaPlugin plugin, FotonCommands commands) {
        PluginState state = stateOf(plugin);
        if (state != null && state.bootstrap != null) state.bootstrap.seedCommands(commands);
    }

    private static boolean isPublishedLocked(Plugin candidate) {
        for (Plugin plugin : loaded) {
            if (plugin == candidate) return true;
        }
        return false;
    }

    private static boolean containsIgnoreCase(List<String> values, String wanted) {
        for (String value : values) {
            if (value.equalsIgnoreCase(wanted)) return true;
        }
        return false;
    }

    private static void unpublishLocked(Plugin plugin) {
        removePublishedLocked(plugin);
        String retired = pluginKey(plugin.getName());
        selectedProviders.entrySet().removeIf(entry -> entry.getValue().equals(retired));
        org.bukkit.plugin.java.PluginClassLoader loader = loadersByPlugin.get(plugin);
        if (loader != null) pluginLoaders.entrySet().removeIf(entry -> entry.getValue() == loader);
        PluginState metadata = statesByPlugin.get(plugin);
        if (metadata != null) states.entrySet().removeIf(entry -> entry.getValue() == metadata);
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
            synchronized (lifecycle) {
                if (loaded.isEmpty()) selectedProviders.clear();
            }
        } finally {
            lifecycleOperation.unlock();
        }
        closeItemBridgeIfDrained();
    }
}
