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
public class PluginDescriptionFile implements io.papermc.paper.plugin.configuration.PluginMeta {
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
    private final List<String> contributors;
    private final PluginLoadOrder load;
    private final String paperPluginLoader;
    private final boolean paperSkipLibraries;
    private final Map<String, Map<String, Object>> commands;
    private final String apiVersion;
    private final String prefix;
    private final String website;
    private final List<org.bukkit.permissions.Permission> permissions;
    private final org.bukkit.permissions.PermissionDefault permissionDefault;

    public PluginDescriptionFile(Reader reader) throws InvalidDescriptionException {
        this(loadMapping(reader));
    }

    private static Map<String, Object> loadMapping(Reader reader) throws InvalidDescriptionException {
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
        return root;
    }

    protected PluginDescriptionFile(Map<String, Object> root) throws InvalidDescriptionException {
        this.name = text(root.get("name"));
        this.main = text(root.get("main"));
        this.version = text(root.get("version"));
        if (name == null || name.isBlank() || main == null || main.isBlank()
                || version == null || version.isBlank()) {
            throw new InvalidDescriptionException("plugin descriptor needs name, main and version");
        }
        if (!name.matches("[A-Za-z0-9 _.-]+")) {
            throw new InvalidDescriptionException("invalid plugin name: " + name);
        }
        this.description = text(root.get("description"));
        this.apiVersion = text(root.get("api-version"));
        this.prefix = text(root.get("prefix"));
        this.website = text(root.get("website"));
        this.authors = names(root.get("authors"), root.get("author"));
        this.depend = stringList(root, "depend");
        this.softDepend = stringList(root, "softdepend");
        this.loadBefore = stringList(root, "loadbefore");
        this.provides = stringList(root, "provides");
        List<String> declaredLibraries = stringList(root, "libraries");
        this.contributors = stringList(root, "contributors");
        try {
            this.load = root.containsKey("load")
                ? PluginLoadOrder.valueOf(text(root.get("load"))) : PluginLoadOrder.POSTWORLD;
        } catch (IllegalArgumentException error) {
            throw new InvalidDescriptionException(error, "invalid plugin load phase");
        }
        this.paperPluginLoader = text(root.get("paper-plugin-loader"));
        Object skip = root.get("paper-skip-libraries");
        this.paperSkipLibraries = skip != null && String.valueOf(skip).equalsIgnoreCase("true");
        this.libraries = paperSkipLibraries ? List.of() : declaredLibraries;
        this.permissionDefault = root.containsKey("default-permission")
            ? permissionDefault(root.get("default-permission")) : org.bukkit.permissions.PermissionDefault.OP;
        this.permissions = permissions(root.get("permissions"), permissionDefault);

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
        this.website = null;
        this.authors = List.of();
        this.depend = List.of();
        this.softDepend = List.of();
        this.loadBefore = List.of();
        this.provides = List.of();
        this.libraries = List.of();
        this.contributors = List.of();
        this.load = PluginLoadOrder.POSTWORLD;
        this.paperPluginLoader = null;
        this.paperSkipLibraries = false;
        this.commands = Map.of();
        this.permissions = List.of();
        this.permissionDefault = org.bukkit.permissions.PermissionDefault.OP;
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
            throw new IllegalArgumentException("cannot read plugin descriptor", error);
        }
        return text.toString();
    }

    private static String text(Object value) {
        return value == null ? null : String.valueOf(value);
    }

    private static List<String> stringList(Map<String, Object> root, String key)
            throws InvalidDescriptionException {
        Object value = root.get(key);
        if (value == null) return List.of();
        if (!(value instanceof List<?> values)) {
            throw new InvalidDescriptionException(key + " must be a list");
        }
        List<String> result = new ArrayList<>();
        for (Object entry : values) {
            if (!(entry instanceof String name) || name.isBlank()) {
                throw new InvalidDescriptionException(key + " must contain nonempty strings");
            }
            result.add(name);
        }
        return List.copyOf(result);
    }

    /** `author: Ada` and `authors: [Ada, Alan]` are both written. */
    private static List<String> names(Object list, Object single) {
        List<String> out = new ArrayList<>();
        if (single != null) out.add(String.valueOf(single));
        if (list instanceof List) {
            for (Object entry : (List<?>) list) {
                if (entry != null) {
                    out.add(String.valueOf(entry));
                }
            }
        } else if (list != null) {
            out.add(String.valueOf(list));
        }
        return Collections.unmodifiableList(out);
    }

    private static List<org.bukkit.permissions.Permission> permissions(Object value,
            org.bukkit.permissions.PermissionDefault inheritedDefault) {
        if (!(value instanceof Map)) return List.of();
        List<org.bukkit.permissions.Permission> out = new ArrayList<>();
        for (Map.Entry<?, ?> entry : ((Map<?, ?>) value).entrySet()) {
            String name = String.valueOf(entry.getKey());
            String description = "";
            org.bukkit.permissions.PermissionDefault defaultValue = inheritedDefault;
            if (entry.getValue() instanceof Map<?, ?> fields) {
                Object d = fields.get("description");
                if (d != null) description = String.valueOf(d);
                Object def = fields.get("default");
                if (def != null) defaultValue = permissionDefault(def);
                Map<String, Boolean> children = permissionChildren(fields.get("children"));
                out.add(new org.bukkit.permissions.Permission(
                    name, description, defaultValue, children));
                continue;
            }
            out.add(new org.bukkit.permissions.Permission(name, description, defaultValue));
        }
        return Collections.unmodifiableList(out);
    }

    private static org.bukkit.permissions.PermissionDefault permissionDefault(Object value) {
        String normalized = String.valueOf(value).trim().toUpperCase(java.util.Locale.ROOT)
            .replace('-', '_').replace(' ', '_');
        try {
            return org.bukkit.permissions.PermissionDefault.valueOf(normalized);
        } catch (IllegalArgumentException ignored) {
            return org.bukkit.permissions.PermissionDefault.FALSE;
        }
    }

    private static Map<String, Boolean> permissionChildren(Object value) {
        if (!(value instanceof Map<?, ?> children)) return Map.of();
        Map<String, Boolean> result = new LinkedHashMap<>();
        for (Map.Entry<?, ?> child : children.entrySet()) {
            Object raw = child.getValue();
            boolean enabled = raw instanceof Boolean bool
                ? bool : Boolean.parseBoolean(String.valueOf(raw));
            result.put(String.valueOf(child.getKey()), enabled);
        }
        return result;
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
    public List<String> getContributors() { return contributors; }
    public PluginLoadOrder getLoad() { return load; }
    public String getPaperPluginLoader() { return paperPluginLoader; }
    public boolean isPaperSkipLibraries() { return paperSkipLibraries; }
    public List<org.bukkit.permissions.Permission> getPermissions() { return permissions; }
    public org.bukkit.permissions.PermissionDefault getPermissionDefault() { return permissionDefault; }

    public String getAPIVersion() { return apiVersion; }

    public String getPrefix() { return prefix; }

    public String getWebsite() { return website; }

    public Map<String, Map<String, Object>> getCommands() { return commands; }

    public String getFullName() { return name + " v" + version; }
}
