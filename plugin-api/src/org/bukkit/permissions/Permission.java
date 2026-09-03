package org.bukkit.permissions;

/** Bukkit permission descriptor. */
public class Permission {
    private final String name;
    private PermissionDefault defaultValue;
    private final String description;
    private final java.util.Map<String, Boolean> children;
    public Permission(String name) { this(name, PermissionDefault.FALSE); }
    public Permission(String name, PermissionDefault defaultValue) {
        this(name, "", defaultValue, java.util.Collections.emptyMap());
    }
    public Permission(String name, String description, PermissionDefault defaultValue) {
        this(name, description, defaultValue, java.util.Collections.emptyMap());
    }
    public Permission(String name, String description, PermissionDefault defaultValue,
                      java.util.Map<String, Boolean> children) {
        this.name = name == null ? "" : name;
        this.description = description == null ? "" : description;
        this.defaultValue = defaultValue == null ? PermissionDefault.FALSE : defaultValue;
        this.children = children == null ? new java.util.LinkedHashMap<>() : new java.util.LinkedHashMap<>(children);
    }
    public String getName() { return name; }
    public PermissionDefault getDefault() { return defaultValue; }
    public void setDefault(PermissionDefault value) {
        defaultValue = value == null ? PermissionDefault.FALSE : value;
    }
    public String getDescription() { return description; }
    public java.util.Map<String, Boolean> getChildren() { return java.util.Collections.unmodifiableMap(children); }

    /** Adds a parent permission as a child rule, matching Bukkit's descriptor semantics. */
    public void addParent(Permission parent, boolean value) {
        if (parent == null) throw new IllegalArgumentException("parent");
        children.put(parent.getName(), value);
    }

    public void addParent(String name, boolean value) {
        if (name == null || name.isEmpty()) throw new IllegalArgumentException("name");
        children.put(name, value);
    }
}
