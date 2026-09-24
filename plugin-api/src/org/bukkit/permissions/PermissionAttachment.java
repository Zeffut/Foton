package org.bukkit.permissions;

import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import org.bukkit.plugin.Plugin;

/** Mutable permission attachment owned by a plugin. */
public class PermissionAttachment {
    private final Plugin plugin;
    private final Permissible permissible;
    private final Map<String, Boolean> permissions = new LinkedHashMap<>();
    public PermissionAttachment(Plugin plugin) { this(plugin, null); }
    public PermissionAttachment(Plugin plugin, Permissible permissible) {
        this.plugin = plugin;
        this.permissible = permissible;
    }
    public Plugin getPlugin() { return plugin; }
    public void setPermission(String name, boolean value) {
        if (name == null) return;
        synchronized (permissions) { permissions.put(name.toLowerCase(Locale.ROOT), value); }
        recalculate();
    }
    public void setPermission(Permission permission, boolean value) { if (permission != null) setPermission(permission.getName(), value); }
    public void unsetPermission(String name) {
        if (name == null) return;
        synchronized (permissions) { permissions.remove(name.toLowerCase(Locale.ROOT)); }
        recalculate();
    }
    public void unsetPermission(Permission permission) { if (permission != null) unsetPermission(permission.getName()); }
    public Map<String, Boolean> getPermissions() {
        synchronized (permissions) {
            return java.util.Collections.unmodifiableMap(new LinkedHashMap<>(permissions));
        }
    }
    public void remove() {
        if (permissible != null) {
            permissible.removeAttachment(this);
            return;
        }
        synchronized (permissions) { permissions.clear(); }
    }

    private void recalculate() {
        if (permissible != null) permissible.recalculatePermissions();
    }
}
