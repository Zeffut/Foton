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
    private static final Map<String, String> selectedProviders = new HashMap<>();
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
        Map<File, PreparedBootstrap> bootstraps = prepareBootstraps(ordered);
        List<JavaPlugin> constructed = new ArrayList<>();
        for (File jar : ordered) {
            PreparedBootstrap prepared = bootstraps.remove(jar);
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                if (descriptor instanceof PaperPluginDescriptor paper
                        && paper.bootstrapper() != null && prepared == null) continue;
                JavaPlugin plugin = constructAndRegister(jar, prepared);
                if (plugin != null) constructed.add(plugin);
            } catch (Throwable error) {
                if (prepared != null) closeUnpublished(prepared.loader());
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
        List<String> bootstrapOrder = new PluginDependencyGraph(descriptors)
            .order(PaperPluginDescriptor.Phase.BOOTSTRAP);
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
                System.out.println("[host] " + jar.getName() + " bootstrap classloader failed: " + error);
                if (loader != null) closeUnpublished(loader);
            }
        }
        List<org.bukkit.plugin.java.PluginClassLoader> failed = new ArrayList<>();
        for (String key : bootstrapOrder) {
            BootstrapCandidate candidate = ready.get(key);
            if (candidate == null) continue;
            try {
                PaperBootstrapRunner runner = new PaperBootstrapRunner(candidate.jar(),
                    candidate.descriptor(), candidate.loader());
                runner.bootstrap();
                prepared.put(candidate.jar(), new PreparedBootstrap(candidate.loader(), runner,
                    candidate.dependencies()));
            } catch (Throwable error) {
                System.out.println("[host] " + candidate.jar().getName() + " bootstrap failed: " + error);
                failed.add(candidate.loader());
            }
        }
        var entries = prepared.entrySet().iterator();
        while (entries.hasNext()) {
            var entry = entries.next();
            try {
                entry.getValue().runner().commands();
            } catch (Throwable error) {
                System.out.println("[host] " + entry.getKey().getName() + " bootstrap commands failed: " + error);
                failed.add(entry.getValue().loader());
                entries.remove();
            }
        }
        // A failed provider remains available to later bootstrap callbacks on
        // Paper. Keep its loader through the command phase, then release it.
        for (var loader : failed) closeUnpublished(loader);
        // Bootstrap visibility ends here. A surviving plugin loader must not
        // retain every sibling (including failed, closed loaders) for its lifetime.
        bootstrapLoaders.clear();
        return prepared;
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
        if (loader.getPlugin() != null) cleanupUnpublished(loader.getPlugin());
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
            boolean failed;
            synchronized (lifecycle) {
                InvocationState state = invocations.get(dependency);
                failed = state != null && state.disableComplete && !dependency.isEnabled();
            }
            if (dependency == null || failed) {
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
        java.util.Arrays.sort(jars, java.util.Comparator.comparing(File::getName));
        for (File jar : jars) {
            try {
                PluginDescriptionFile descriptor = readDescriptor(jar);
                requireSupportedLoading(descriptor);
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
        PluginDependencyGraph graph = new PluginDependencyGraph(descriptors.values());
        synchronized (lifecycle) {
            // A deferred teardown may have completed since disableAll returned.
            // An empty host starts a fresh provider selection, including aliases.
            if (loaded.isEmpty()) selectedProviders.clear();
            selectedProviders.putAll(graph.providers());
        }
        // Bootstrap has its own node set and ordering; it never contributes
        // runtime classpath or enable-order edges.
        graph.order(PaperPluginDescriptor.Phase.BOOTSTRAP);
        List<File> ordered = new ArrayList<>();
        for (String key : graph.order(PaperPluginDescriptor.Phase.SERVER)) {
            ordered.add(jarsByName.get(key));
        }
        return ordered;
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

    private static JavaPlugin constructAndRegister(File jar, PreparedBootstrap prepared) throws Exception {
        PluginDescriptionFile descriptor = readDescriptor(jar);
        requireSupportedLoading(descriptor);
        synchronized (lifecycle) {
            if (findExactNameLocked(descriptor.getName()) != null) {
                if (prepared != null) closeUnpublished(prepared.loader());
                System.out.println("[host] " + descriptor.getName() + " is already loaded; skipping " + jar.getName());
                return null;
            }
        }
        org.bukkit.plugin.java.PluginClassLoader loader = prepared == null
            ? new org.bukkit.plugin.java.PluginClassLoader(pluginUrls(jar, descriptor), dependencyParent(descriptor))
            : prepared.loader();
        boolean registered = false;
        JavaPlugin plugin = null;
        try {
            File dataFolder = new File(jar.getParentFile(), descriptor.getName());
            // Before the constructor, not after: a plugin may call getName()
            // or getLogger() from it, and several do.
            loader.describe(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            PaperBootstrapRunner bootstrap = prepared == null ? null : prepared.runner();
            Object instance;
            if (bootstrap != null) {
                prepared.dependencies().useServerDependencies(
                    PluginDependencyGraph.visible(descriptor, PaperPluginDescriptor.Phase.SERVER));
                instance = bootstrap.createPlugin();
            } else {
                Class<?> type = Class.forName(descriptor.getMain(), true, loader);
                instance = type.getDeclaredConstructor().newInstance();
            }
            if (!(instance instanceof JavaPlugin candidate)) {
                System.out.println(
                    "[host] " + descriptor.getName() + ": main class is not a JavaPlugin");
                return null;
            }
            if (bootstrap != null && (candidate.getClass().getClassLoader() != loader || loader.getPlugin() != candidate)) {
                throw new IllegalStateException("createPlugin must return the JavaPlugin constructed by its own loader");
            }
            plugin = candidate;

            loader.setPlugin(plugin);
            plugin.init(org.bukkit.Bukkit.getServer(), descriptor, dataFolder);
            synchronized (lifecycle) {
                if (findExactNameLocked(descriptor.getName()) != null) {
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
            PermissionRegistry.register(plugin, descriptor.getPermissions());
            if (bootstrap != null) bootstrap.publishCommands(plugin);
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
        PermissionRegistry.removeOwnedBy(plugin);
        plugin.setEnabled(false);
        org.bukkit.Bukkit.getScheduler().cancelTasks(plugin);
        FotonPlayer.removeAttachments(plugin);
        CommandMap.forget(plugin);
        org.bukkit.Bukkit.getServicesManager().unregisterAll(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterIncomingPluginChannel(plugin);
        org.bukkit.Bukkit.getMessenger().unregisterOutgoingPluginChannel(plugin);
        EventBridge.unregister(plugin);
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
                resolver.addDependency(new org.eclipse.aether.graph.Dependency(new org.eclipse.aether.artifact.DefaultArtifact(coordinate), null));
            }
            legacyLibraries = resolver;
        }
        String loaderName = descriptor instanceof PaperPluginDescriptor paper ? paper.loader() : descriptor.getPaperPluginLoader();
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
        PermissionRegistry.removeOwnedBy(plugin);
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
        String selected = selectedProviders.get(pluginKey(name));
        return findExactNameLocked(selected == null ? name : selected);
    }

    private static Plugin findExactNameLocked(String name) {
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
            synchronized (lifecycle) {
                if (loaded.isEmpty()) selectedProviders.clear();
            }
        } finally {
            lifecycleOperation.unlock();
        }
    }
}
