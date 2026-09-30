package org.bukkit.plugin;

import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.Reader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;

/** What a plugin.yml said.
 *
 * Plugins read their own descriptor -- for the version they print on enable,
 * for the author list in an /about command, and for the command map they were
 * declared with -- so this holds the file rather than three fields from it.
 */
public class PluginDescriptionFile implements io.papermc.paper.plugin.configuration.PluginMeta {
    public enum Load { BEFORE, AFTER, OMIT }

    public record Dependency(Load load, boolean required, boolean joinClasspath) {
        public Dependency {
            if (load == null) {
                throw new IllegalArgumentException("load cannot be null");
            }
        }
    }

    private static final int[] MAX_API_VERSION = {26, 2};

    private final String name;
    private final String version;
    private final String main;
    private final String description;
    private final List<String> authors;
    private final List<String> depend;
    private final List<String> softDepend;
    private final List<String> loadBefore;
    private final List<String> provides;
    private final List<String> libraries;
    private final Map<String, Map<String, Object>> commands;
    private final String apiVersion;
    private final String bootstrapper;
    private final String loader;
    private final Map<String, Dependency> bootstrapDependencies;
    private final Map<String, Dependency> serverDependencies;
    private final String prefix;
    private final String website;
    private final List<org.bukkit.permissions.Permission> permissions;

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

        this.name = requiredText(root, "name");
        this.main = requiredText(root, "main");
        this.version = root.containsKey("version") ? text(root.get("version")) : "0";
        this.description = text(root.get("description"));
        this.apiVersion = apiVersion(root.get("api-version"));
        this.prefix = text(root.get("prefix"));
        this.website = text(root.get("website"));
        this.authors = names(root.get("authors"), root.get("author"));
        this.depend = dependencyNames(root.get("depend"), "depend");
        this.softDepend = dependencyNames(root.get("softdepend"), "softdepend");
        this.loadBefore = dependencyNames(root.get("loadbefore"), "loadbefore");
        this.provides = dependencyNames(root.get("provides"), "provides");
        rejectOwnAlias();
        this.libraries = dependencyNames(root.get("libraries"), "libraries");
        this.bootstrapper = optionalText(root.get("bootstrapper"), "bootstrapper");
        this.loader = optionalText(root.get("loader"), "loader");
        Map<String, Object> dependencies = mapping(root.get("dependencies"), "dependencies", true);
        this.bootstrapDependencies = dependencySection(
            dependencies.get("bootstrap"), "dependencies.bootstrap");
        this.serverDependencies = dependencySection(
            dependencies.get("server"), "dependencies.server");
        this.permissions = permissions(root.get("permissions"));

        Map<String, Map<String, Object>> declared = new LinkedHashMap<>();
        if (root.get("commands") instanceof Map) {
            for (Map.Entry<?, ?> entry : ((Map<?, ?>) root.get("commands")).entrySet()) {
                Map<String, Object> body = new LinkedHashMap<>();
                if (entry.getValue() instanceof Map) {
                    for (Map.Entry<?, ?> field : ((Map<?, ?>) entry.getValue()).entrySet()) {
                        body.put(String.valueOf(field.getKey()), field.getValue());
                    }
                }
                declared.put(String.valueOf(entry.getKey()),
                    Collections.unmodifiableMap(body));
            }
        }
        this.commands = Collections.unmodifiableMap(declared);
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
        this.website = null;
        this.authors = List.of();
        this.depend = List.of();
        this.softDepend = List.of();
        this.loadBefore = List.of();
        this.provides = List.of();
        this.libraries = List.of();
        this.commands = Map.of();
        this.bootstrapper = null;
        this.loader = null;
        this.bootstrapDependencies = Map.of();
        this.serverDependencies = Map.of();
        this.permissions = List.of();
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

    private static String requiredText(Map<String, Object> root, String field)
            throws InvalidDescriptionException {
        String value = optionalText(root.get(field), field);
        if (value == null) {
            throw new InvalidDescriptionException("plugin.yml field " + field + " is required");
        }
        return value;
    }

    private static String optionalText(Object raw, String field)
            throws InvalidDescriptionException {
        if (raw == null) return null;
        String value = String.valueOf(raw).trim();
        if (value.isEmpty()) {
            throw new InvalidDescriptionException("plugin.yml field " + field + " cannot be blank");
        }
        return value;
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

    private static List<String> dependencyNames(Object raw, String field)
            throws InvalidDescriptionException {
        List<String> values = names(raw, null);
        List<String> out = new ArrayList<>();
        Set<String> seen = new HashSet<>();
        for (String rawName : values) {
            String value = rawName.trim();
            if (value.isEmpty()) {
                throw new InvalidDescriptionException(
                    "plugin.yml field " + field + " contains a blank name");
            }
            if (!seen.add(value.toLowerCase(Locale.ROOT))) {
                throw new InvalidDescriptionException(
                    "plugin.yml field " + field + " contains duplicate name " + value);
            }
            out.add(value);
        }
        return Collections.unmodifiableList(out);
    }

    private void rejectOwnAlias() throws InvalidDescriptionException {
        for (String alias : provides) {
            if (alias.equalsIgnoreCase(name)) {
                throw new InvalidDescriptionException(
                    "plugin.yml field provides duplicates plugin name " + name);
            }
        }
    }

    private static String apiVersion(Object raw) throws InvalidDescriptionException {
        String version = optionalText(raw, "api-version");
        if (version == null) return null;
        String[] parts = version.split("\\.", -1);
        int[] parsed = new int[parts.length];
        try {
            for (int index = 0; index < parts.length; index++) {
                if (parts[index].isEmpty()) throw new NumberFormatException();
                parsed[index] = Integer.parseInt(parts[index]);
                if (parsed[index] < 0) throw new NumberFormatException();
            }
        } catch (NumberFormatException error) {
            throw new InvalidDescriptionException(
                "plugin.yml field api-version is not numeric: " + version);
        }
        int width = Math.max(parsed.length, MAX_API_VERSION.length);
        for (int index = 0; index < width; index++) {
            int requested = index < parsed.length ? parsed[index] : 0;
            int supported = index < MAX_API_VERSION.length ? MAX_API_VERSION[index] : 0;
            if (requested < supported) return version;
            if (requested > supported) {
                throw new InvalidDescriptionException(
                    "plugin.yml field api-version " + version + " is newer than 26.2");
            }
        }
        return version;
    }

    private static Map<String, Object> mapping(
            Object raw, String field, boolean absentAllowed)
            throws InvalidDescriptionException {
        if (raw == null && absentAllowed) return Map.of();
        if (!(raw instanceof Map<?, ?> values)) {
            throw new InvalidDescriptionException(
                "plugin.yml field " + field + " must be a mapping");
        }
        Map<String, Object> out = new LinkedHashMap<>();
        for (Map.Entry<?, ?> entry : values.entrySet()) {
            out.put(String.valueOf(entry.getKey()), entry.getValue());
        }
        return out;
    }

    private static Map<String, Dependency> dependencySection(Object raw, String field)
            throws InvalidDescriptionException {
        Map<String, Object> entries = mapping(raw, field, true);
        Map<String, Dependency> out = new LinkedHashMap<>();
        Set<String> seen = new HashSet<>();
        for (Map.Entry<String, Object> entry : entries.entrySet()) {
            String name = entry.getKey().trim();
            if (name.isEmpty()) {
                throw new InvalidDescriptionException(
                    "plugin.yml field " + field + " contains a blank name");
            }
            if (!seen.add(name.toLowerCase(Locale.ROOT))) {
                throw new InvalidDescriptionException(
                    "plugin.yml field " + field + " contains duplicate name " + name);
            }
            String dependencyField = field + "." + name;
            Map<String, Object> values = mapping(entry.getValue(), dependencyField, false);
            Load load = load(values.get("load"), dependencyField + ".load");
            boolean required = bool(values.get("required"), true,
                dependencyField + ".required");
            boolean joinClasspath = bool(values.get("join-classpath"), true,
                dependencyField + ".join-classpath");
            out.put(name, new Dependency(load, required, joinClasspath));
        }
        return Collections.unmodifiableMap(out);
    }

    private static Load load(Object raw, String field) throws InvalidDescriptionException {
        if (raw == null) return Load.OMIT;
        try {
            return Load.valueOf(String.valueOf(raw).trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException error) {
            throw new InvalidDescriptionException(
                "plugin.yml field " + field + " must be BEFORE, AFTER, or OMIT");
        }
    }

    private static boolean bool(Object raw, boolean fallback, String field)
            throws InvalidDescriptionException {
        if (raw == null) return fallback;
        if (raw instanceof Boolean value) return value;
        if ("true".equalsIgnoreCase(String.valueOf(raw))) return true;
        if ("false".equalsIgnoreCase(String.valueOf(raw))) return false;
        throw new InvalidDescriptionException(
            "plugin.yml field " + field + " must be true or false");
    }

    private static List<org.bukkit.permissions.Permission> permissions(Object value) {
        if (!(value instanceof Map)) return List.of();
        List<org.bukkit.permissions.Permission> out = new ArrayList<>();
        for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
            String name = String.valueOf(entry.getKey());
            String description = "";
            org.bukkit.permissions.PermissionDefault defaultValue = org.bukkit.permissions.PermissionDefault.FALSE;
            if (entry.getValue() instanceof Map<?, ?> fields) {
                Object d = fields.get("description");
                if (d != null) description = String.valueOf(d);
                Object def = fields.get("default");
                if (def != null) try { defaultValue = org.bukkit.permissions.PermissionDefault.valueOf(String.valueOf(def).toUpperCase(java.util.Locale.ROOT)); } catch (IllegalArgumentException ignored) { }
            }
            out.add(new org.bukkit.permissions.Permission(name, description, defaultValue));
        }
        return Collections.unmodifiableList(out);
    }

    public String getName() { return name; }

    @Override public String getVersion() { return version; }

    public String getMain() { return main; }

    public String getDescription() { return description; }

    public List<String> getAuthors() { return authors; }

    public List<String> getDepend() { return depend; }

    public List<String> getSoftDepend() { return softDepend; }

    public List<String> getLoadBefore() { return loadBefore; }

    public List<String> getProvides() { return provides; }

    public List<String> getLibraries() { return libraries; }

    public String getBootstrapper() { return bootstrapper; }

    public String getLoader() { return loader; }

    public Map<String, Dependency> getBootstrapDependencies() {
        return bootstrapDependencies;
    }

    public Map<String, Dependency> getServerDependencies() { return serverDependencies; }

    public List<org.bukkit.permissions.Permission> getPermissions() { return permissions; }

    public String getAPIVersion() { return apiVersion; }

    public String getPrefix() { return prefix; }

    public String getWebsite() { return website; }

    public Map<String, Map<String, Object>> getCommands() { return commands; }

    public String getFullName() { return name + " v" + version; }
}
