package foton;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.bukkit.plugin.InvalidDescriptionException;
import org.bukkit.plugin.PluginDescriptionFile;

/** Parsed metadata for the deliberately different {@code paper-plugin.yml} format. */
final class PaperPluginDescriptor extends PluginDescriptionFile {
    enum LoadOrder { BEFORE, AFTER, OMIT }
    enum Phase { BOOTSTRAP, SERVER }

    record Dependency(String name, Phase phase, LoadOrder load, boolean required,
                      boolean joinClasspath) {}

    private final String bootstrapper;
    private final String loader;
    private final boolean openClassloader;
    private final List<String> authors;
    private final List<Dependency> dependencies;
    private final ApiVersion apiVersion;

    private PaperPluginDescriptor(Map<String, Object> root, List<Dependency> dependencies)
            throws InvalidDescriptionException {
        super(commonFields(root));
        this.apiVersion = ApiVersion.parse(requiredText(root, "api-version"));
        if (apiVersion.compareTo(new ApiVersion(1, 19, 0)) < 0) {
            throw new InvalidDescriptionException("Paper api-version must be at least 1.19");
        }
        this.bootstrapper = text(root.get("bootstrapper"));
        this.loader = text(root.get("loader"));
        this.openClassloader = booleanValue(root.get("has-open-classloader"), false);
        List<String> declaredAuthors = new ArrayList<>(super.getAuthors());
        if (root.get("author") != null && declaredAuthors.size() > 1) {
            declaredAuthors.add(declaredAuthors.remove(0));
        }
        this.authors = List.copyOf(declaredAuthors);
        this.dependencies = List.copyOf(dependencies);
    }

    private static Map<String, Object> commonFields(Map<String, Object> root) {
        Map<String, Object> fields = new LinkedHashMap<>(root);
        // These keys belong only to the Bukkit descriptor, even in a dual-descriptor jar.
        for (String key : List.of("depend", "softdepend", "loadbefore", "commands",
                "libraries", "paper-plugin-loader", "paper-skip-libraries")) fields.remove(key);
        // Paper validates grammar here and the target version before discovery;
        // legacy descriptors keep their existing parser-time target bound.
        fields.remove("api-version");
        return fields;
    }

    static PaperPluginDescriptor read(InputStream stream) throws InvalidDescriptionException {
        Object loaded;
        try {
            loaded = Yaml.load(readText(new InputStreamReader(stream, StandardCharsets.UTF_8)));
        } catch (RuntimeException error) {
            throw new InvalidDescriptionException(error);
        }
        if (!(loaded instanceof Map<?, ?> source)) {
            throw new InvalidDescriptionException("paper-plugin.yml is not a mapping");
        }
        Map<String, Object> root = stringMap(source);
        String name = requiredText(root, "name");
        if (name.contains(" ") || java.util.Set.of("bukkit", "minecraft", "mojang", "spigot", "paper")
                .contains(name.toLowerCase(java.util.Locale.ROOT))) {
            throw new InvalidDescriptionException("restricted Paper plugin name: " + name);
        }
        requiredText(root, "version");
        requiredText(root, "main");
        requiredText(root, "api-version");
        for (String key : List.of("main", "bootstrapper", "loader")) {
            String className = text(root.get(key));
            if (className == null) continue;
            for (String prefix : List.of("net.minecraft.", "org.bukkit.", "io.papermc.paper.",
                    "com.destroystokoyo.paper.")) {
                if (className.startsWith(prefix)) {
                    throw new InvalidDescriptionException("restricted Paper plugin namespace: " + className);
                }
            }
        }
        List<Dependency> dependencies = new ArrayList<>();
        Object declaredDependencies = root.get("dependencies");
        if (declaredDependencies != null) {
            if (!(declaredDependencies instanceof Map<?, ?> sections)) {
                throw new InvalidDescriptionException("paper-plugin.yml dependencies is not a mapping");
            }
            parseDependencies(sections, "bootstrap", Phase.BOOTSTRAP, dependencies);
            parseDependencies(sections, "server", Phase.SERVER, dependencies);
        }
        return new PaperPluginDescriptor(root, dependencies);
    }

    private static void parseDependencies(Map<?, ?> sections, String key, Phase phase,
            List<Dependency> output) throws InvalidDescriptionException {
        Object section = sections.get(key);
        if (section == null) return;
        if (!(section instanceof Map<?, ?> entries)) {
            throw new InvalidDescriptionException("paper-plugin.yml dependencies." + key
                + " is not a mapping");
        }
        for (Map.Entry<?, ?> entry : entries.entrySet()) {
            String name = String.valueOf(entry.getKey()).trim();
            if (name.isEmpty()) {
                throw new InvalidDescriptionException("paper-plugin.yml has an empty dependency name");
            }
            Map<String, Object> fields;
            if (entry.getValue() == null) fields = Map.of();
            else if (entry.getValue() instanceof Map<?, ?> map) fields = stringMap(map);
            else throw new InvalidDescriptionException("paper dependency " + name
                + " is not a mapping");
            LoadOrder order;
            try {
                order = fields.containsKey("load")
                    ? LoadOrder.valueOf(String.valueOf(fields.get("load")).toUpperCase(java.util.Locale.ROOT))
                    : LoadOrder.OMIT;
            } catch (IllegalArgumentException error) {
                throw new InvalidDescriptionException(error, "paper dependency " + name
                    + " has invalid load order");
            }
            output.add(new Dependency(name, phase, order,
                booleanValue(fields.get("required"), true),
                booleanValue(fields.get("join-classpath"), true)));
        }
    }

    private static boolean booleanValue(Object value, boolean fallback)
            throws InvalidDescriptionException {
        if (value == null) return fallback;
        if (value instanceof Boolean flag) return flag;
        String text = String.valueOf(value);
        if (text.equalsIgnoreCase("true")) return true;
        if (text.equalsIgnoreCase("false")) return false;
        throw new InvalidDescriptionException("expected a boolean, got " + text);
    }

    private static String requiredText(Map<String, Object> root, String key)
            throws InvalidDescriptionException {
        String value = text(root.get(key));
        if (value == null || value.isBlank()) {
            throw new InvalidDescriptionException("paper-plugin.yml needs " + key);
        }
        return value;
    }

    private static Map<String, Object> stringMap(Map<?, ?> source) {
        Map<String, Object> output = new LinkedHashMap<>();
        for (Map.Entry<?, ?> entry : source.entrySet()) {
            output.put(String.valueOf(entry.getKey()), entry.getValue());
        }
        return output;
    }

    private static String readText(Reader reader) {
        StringBuilder output = new StringBuilder();
        char[] buffer = new char[4096];
        try {
            int count;
            while ((count = reader.read(buffer)) >= 0) output.append(buffer, 0, count);
        } catch (java.io.IOException error) {
            throw new IllegalArgumentException("cannot read paper-plugin.yml", error);
        }
        return output.toString();
    }

    private static String text(Object value) {
        return value == null ? null : String.valueOf(value);
    }

    String bootstrapper() { return bootstrapper; }
    String loader() { return loader; }
    boolean hasOpenClassloader() { return openClassloader; }
    @Override public List<String> getAuthors() { return authors; }
    @Override public String getAPIVersion() { return apiVersion.toString(); }
    List<Dependency> paperDependencies() { return dependencies; }

    void validateTarget(String target) throws InvalidDescriptionException {
        if (apiVersion.compareTo(ApiVersion.parse(target)) > 0) {
            throw new InvalidDescriptionException("Unsupported API version " + apiVersion
                + "; server target is " + target);
        }
    }

    /** Mirrors Paper 26.2 ApiVersion parsing and normalization. */
    private record ApiVersion(int major, int minor, int patch) implements Comparable<ApiVersion> {
        static ApiVersion parse(String value) throws InvalidDescriptionException {
            if (value == null || value.trim().isEmpty() || value.equalsIgnoreCase("none")) {
                return new ApiVersion(Integer.MIN_VALUE, Integer.MIN_VALUE, Integer.MIN_VALUE);
            }
            String[] parts = value.split("\\.");
            if (parts.length != 2 && parts.length != 3) {
                throw new InvalidDescriptionException("invalid API version: " + value);
            }
            try {
                return new ApiVersion(Integer.parseInt(parts[0]), Integer.parseInt(parts[1]),
                    parts.length == 3 ? Integer.parseInt(parts[2]) : 0);
            } catch (NumberFormatException error) {
                throw new InvalidDescriptionException(error, "invalid API version: " + value);
            }
        }

        @Override public int compareTo(ApiVersion other) {
            int order = Integer.compare(major, other.major);
            if (order == 0) order = Integer.compare(minor, other.minor);
            if (order == 0) order = Integer.compare(patch, other.patch);
            return order;
        }

        @Override public String toString() { return major + "." + minor + "." + patch; }
    }

}
