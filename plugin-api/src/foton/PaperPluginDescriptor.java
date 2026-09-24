package foton;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
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

    private final String description;
    private final String apiVersion;
    private final String bootstrapper;
    private final String loader;
    private final List<String> authors;
    private final List<Dependency> dependencies;

    private PaperPluginDescriptor(String name, String version, String main,
            String description, String apiVersion, String bootstrapper, String loader,
            List<String> authors, List<Dependency> dependencies) {
        super(name, version, main);
        this.description = description;
        this.apiVersion = apiVersion;
        this.bootstrapper = bootstrapper;
        this.loader = loader;
        this.authors = List.copyOf(authors);
        this.dependencies = List.copyOf(dependencies);
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
        String version = requiredText(root, "version");
        String main = requiredText(root, "main");
        List<Dependency> dependencies = new ArrayList<>();
        Object declaredDependencies = root.get("dependencies");
        if (declaredDependencies != null) {
            if (!(declaredDependencies instanceof Map<?, ?> sections)) {
                throw new InvalidDescriptionException("paper-plugin.yml dependencies is not a mapping");
            }
            parseDependencies(sections, "bootstrap", Phase.BOOTSTRAP, dependencies);
            parseDependencies(sections, "server", Phase.SERVER, dependencies);
        }
        return new PaperPluginDescriptor(name, version, main, text(root.get("description")),
            text(root.get("api-version")), text(root.get("bootstrapper")),
            text(root.get("loader")), names(root.get("authors"), root.get("author")),
            dependencies);
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

    private static List<String> names(Object values, Object single) {
        List<String> output = new ArrayList<>();
        if (values instanceof List<?> list) {
            for (Object value : list) if (value != null) output.add(String.valueOf(value));
        } else if (values != null) output.add(String.valueOf(values));
        if (single != null) output.add(String.valueOf(single));
        return Collections.unmodifiableList(output);
    }

    String bootstrapper() { return bootstrapper; }
    String loader() { return loader; }
    List<Dependency> paperDependencies() { return dependencies; }

    @Override public String getDescription() { return description; }
    @Override public String getAPIVersion() { return apiVersion; }
    @Override public List<String> getAuthors() { return authors; }
}
