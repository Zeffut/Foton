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
        recalculatePermissibles();
    }
    public String getDescription() { return description; }
    public java.util.Map<String, Boolean> getChildren() { return children; }

    /** Adds this permission to the parent's child rules, matching Bukkit. */
    public void addParent(Permission parent, boolean value) {
        if (parent == null) throw new IllegalArgumentException("parent");
        parent.children.put(name, value);
        parent.recalculatePermissibles();
    }

    public Permission addParent(String name, boolean value) {
        if (name == null || name.isEmpty()) throw new IllegalArgumentException("name");
        String normalized = name.toLowerCase(java.util.Locale.ROOT);
        org.bukkit.plugin.PluginManager manager = org.bukkit.Bukkit.getPluginManager();
        Permission parent = manager.getPermission(normalized);
        if (parent == null) {
            parent = new Permission(normalized);
            manager.addPermission(parent);
        }
        addParent(parent, value);
        return parent;
    }

    /** Invalidates the cached effective permissions of every live permissible. */
    public void recalculatePermissibles() {
        foton.PermissionRegistry.changed();
    }
}
