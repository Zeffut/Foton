package io.papermc.paper.plugin.loader.library.impl;

import io.papermc.paper.plugin.loader.library.ClassPathLibrary;
import io.papermc.paper.plugin.loader.library.LibraryLoadingException;
import io.papermc.paper.plugin.loader.library.LibraryStore;
import java.io.IOException;
import java.nio.file.Path;

/** Adds an existing local jar to a Paper plugin's isolated class path. */
public final class JarLibrary implements ClassPathLibrary {
    private final Path path;

    public JarLibrary(Path path) {
        if (path == null) throw new IllegalArgumentException("path");
        this.path = path;
    }

    @Override public void register(LibraryStore store) throws LibraryLoadingException {
        try {
            store.addLibrary(path.toRealPath());
        } catch (IOException error) {
            throw new LibraryLoadingException("Cannot read local plugin library " + path, error);
        }
    }
}
