package io.papermc.paper.plugin.loader.library;

/** A loader library could not be resolved into a safe local jar. */
public class LibraryLoadingException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public LibraryLoadingException(String message) { super(message); }
    public LibraryLoadingException(String message, Throwable cause) { super(message, cause); }
}
