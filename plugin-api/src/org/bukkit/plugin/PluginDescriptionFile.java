package org.bukkit.plugin;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** What a plugin.yml said.
 *
 * Plugins read their own descriptor -- for the version they print on enable,
 * for the author list in an /about command, and for the command map they were
 * declared with -- so this holds the file rather than three fields from it.
 */
public final class PluginDescriptionFile {
    private final String name;
    private final String version;
    private final String main;
    private final String description;
    private final List<String> authors;
    private final List<String> depend;
    private final List<String> softDepend;
    private final Map<String, Map<String, Object>> commands;
    private final String apiVersion;
    private final String prefix;

    @SuppressWarnings("unchecked")
    public PluginDescriptionFile(Reader reader) throws InvalidDescriptionException {
        Object loaded;
        try {
            loaded = foton.Yaml.load(read(reader));
        } catch (RuntimeException error) {
            throw new InvalidDescriptionException(error);
        }
        if (!(loaded instanceof Map)) {
            throw new InvalidDescriptionException("plugin.yml is not a mapping");
        }
        Map<String, Object> root = new LinkedHashMap<>();
        for (Map.Entry<?, ?> entry : ((Map<?, ?>) loaded).entrySet()) {
            root.put(String.valueOf(entry.getKey()), entry.getValue());
        }

        this.name = text(root.get("name"));
        this.main = text(root.get("main"));
        if (name == null || main == null) {
            throw new InvalidDescriptionException("plugin.yml needs both name and main");
        }
        this.version = root.containsKey("version") ? text(root.get("version")) : "0";
        this.description = text(root.get("description"));
        this.apiVersion = text(root.get("api-version"));
        this.prefix = text(root.get("prefix"));
        this.authors = names(root.get("authors"), root.get("author"));
        this.depend = names(root.get("depend"), null);
        this.softDepend = names(root.get("softdepend"), null);

        Map<String, Map<String, Object>> declared = new LinkedHashMap<>();
        if (root.get("commands") instanceof Map) {
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) root.get("commands")).entrySet()) {
                Map<String, Object> body = new LinkedHashMap<>();
                if (entry.getValue() instanceof Map) {
                    for (Map.Entry<?, ?> field : ((Map<?, ?>) entry.getValue()).entrySet()) {
                        body.put(String.valueOf(field.getKey()), field.getValue());
                    }
                }
                declared.put(String.valueOf(entry.getKey()), body);
            }
        }
        this.commands = declared;
    }

    public PluginDescriptionFile(InputStream stream) throws InvalidDescriptionException {
        this(new InputStreamReader(stream, StandardCharsets.UTF_8));
    }

    public PluginDescriptionFile(String name, String version, String main) {
        this.name = name;
        this.version = version;
        this.main = main;
        this.description = null;
        this.apiVersion = null;
        this.prefix = null;
        this.authors = List.of();
        this.depend = List.of();
        this.softDepend = List.of();
        this.commands = Map.of();
    }

    private static String read(Reader reader) {
        StringBuilder text = new StringBuilder();
        char[] buffer = new char[4096];
        try {
            int got;
            while ((got = reader.read(buffer)) != -1) {
                text.append(buffer, 0, got);
            }
        } catch (java.io.IOException error) {
            return "";
        }
        return text.toString();
    }

    private static String text(Object value) {
        return value == null ? null : String.valueOf(value);
    }

    /** `author: Ada` and `authors: [Ada, Alan]` are both written. */
    private static List<String> names(Object list, Object single) {
        List<String> out = new ArrayList<>();
        if (list instanceof List) {
            for (Object entry : (List<?>) list) {
                if (entry != null) {
                    out.add(String.valueOf(entry));
                }
            }
        } else if (list != null) {
            out.add(String.valueOf(list));
        }
        if (single != null) {
            out.add(String.valueOf(single));
        }
        return Collections.unmodifiableList(out);
    }

    public String getName() { return name; }

    public String getVersion() { return version; }

    public String getMain() { return main; }

    public String getDescription() { return description; }

    public List<String> getAuthors() { return authors; }

    public List<String> getDepend() { return depend; }

    public List<String> getSoftDepend() { return softDepend; }

    public String getAPIVersion() { return apiVersion; }

    public String getPrefix() { return prefix; }

    public Map<String, Map<String, Object>> getCommands() { return commands; }

    public String getFullName() { return name + " v" + version; }
}
