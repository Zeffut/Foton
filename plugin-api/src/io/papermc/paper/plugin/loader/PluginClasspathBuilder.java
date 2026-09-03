package io.papermc.paper.plugin.loader;

import java.util.ArrayList;
import java.util.List;
import io.papermc.paper.plugin.loader.library.ClassPathLibrary;

/** What a plugin's loader adds to its class path before the plugin loads.
 *
 * The libraries are recorded and nothing is fetched. Paper resolves them from
 * Maven; Foton has no resolver, so a plugin whose classes need a library it
 * asked for here will fail to find it. That is stated rather than hidden: the
 * alternative is a plugin that loads and then throws NoClassDefFoundError
 * somewhere unrelated.
 */
public final class PluginClasspathBuilder {
    private final List<ClassPathLibrary> libraries = new ArrayList<>();

    public PluginClasspathBuilder addLibrary(ClassPathLibrary library) {
        if (library != null) {
            libraries.add(library);
            System.out.println("[host] a plugin asked for a library; Foton resolves none, so "
                + "anything it needs from that library will be missing");
        }
        return this;
    }

    public List<ClassPathLibrary> getLibraries() {
        return libraries;
    }
}
