package org.bukkit.permissions;

import java.util.LinkedHashMap;
import java.util.Map;
import org.bukkit.plugin.Plugin;

/** Mutable permission attachment owned by a plugin. */
public class PermissionAttachment {
    private final Plugin plugin;
    private final Map<String, Boolean> permissions = new LinkedHashMap<>();
    public PermissionAttachment(Plugin plugin) { this.plugin = plugin; }
    public Plugin getPlugin() { return plugin; }
    public void setPermission(String name, boolean value) { if (name != null) permissions.put(name, value); }
    public void setPermission(Permission permission, boolean value) { if (permission != null) setPermission(permission.getName(), value); }
    public void unsetPermission(String name) { permissions.remove(name); }
    public void unsetPermission(Permission permission) { if (permission != null) unsetPermission(permission.getName()); }
    public Map<String, Boolean> getPermissions() { return java.util.Collections.unmodifiableMap(permissions); }
    public void remove() { permissions.clear(); }
}
