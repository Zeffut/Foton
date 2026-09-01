package org.bukkit.plugin;

/** What a plugin.yml said. */
public final class PluginDescriptionFile {
    private final String name;
    private final String version;
    private final String main;

    public PluginDescriptionFile(String name, String version, String main) {
        this.name = name;
        this.version = version;
        this.main = main;
    }

    public String getName() { return name; }
    public String getVersion() { return version; }
    public String getMain() { return main; }
    public String getFullName() { return name + " v" + version; }
}
