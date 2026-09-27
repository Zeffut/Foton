package io.papermc.paper.plugin.loader.library.impl;

import foton.PluginLibraryResolver;
import io.papermc.paper.plugin.loader.library.ClassPathLibrary;
import io.papermc.paper.plugin.loader.library.LibraryLoadingException;
import io.papermc.paper.plugin.loader.library.LibraryStore;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.repository.RemoteRepository;

/** Paper's Maven loader ABI backed by the shared bounded Maven resolver. */
public class MavenLibraryResolver implements ClassPathLibrary {
    public static final String MAVEN_CENTRAL_DEFAULT_MIRROR = defaultMirror();
    private final List<Dependency> dependencies = new ArrayList<>();
    private final List<RemoteRepository> repositories = new ArrayList<>();

    public MavenLibraryResolver() {}
    public void addDependency(Dependency dependency) { dependencies.add(java.util.Objects.requireNonNull(dependency)); }
    public void addRepository(RemoteRepository repository) { repositories.add(java.util.Objects.requireNonNull(repository)); }

    @Override public void register(LibraryStore store) throws LibraryLoadingException {
        try {
            for (var url : PluginLibraryResolver.resolve(PluginLibraryResolver.cacheDirectory(), dependencies, repositories)) {
                store.addLibrary(Path.of(url.toURI()));
            }
        } catch (Exception error) { throw new LibraryLoadingException("Error resolving plugin libraries", error); }
    }

    private static String defaultMirror() {
        String mirror = System.getenv("PAPER_DEFAULT_CENTRAL_REPOSITORY");
        if (mirror == null) mirror = System.getProperty("org.bukkit.plugin.java.LibraryLoader.centralURL");
        return mirror == null ? "https://maven-central.storage-download.googleapis.com/maven2" : mirror;
    }
}
