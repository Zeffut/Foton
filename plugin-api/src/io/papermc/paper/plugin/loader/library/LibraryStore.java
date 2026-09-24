package io.papermc.paper.plugin.loader.library;

import java.nio.file.Path;

/** Receives validated paths that will enter one plugin's isolated class path. */
public interface LibraryStore {
    void addLibrary(Path library);
}
