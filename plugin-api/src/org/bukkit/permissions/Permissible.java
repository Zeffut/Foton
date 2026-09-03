package org.bukkit.permissions;

/** Object whose permission state can be queried. */
public interface Permissible {
    boolean hasPermission(String permission);
    default boolean hasPermission(Permission permission) {
        return permission != null && hasPermission(permission.getName());
    }
    default PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin) { return new PermissionAttachment(plugin); }
    default PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, int ticks) { return addAttachment(plugin); }
    default PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value) { PermissionAttachment attachment = addAttachment(plugin); attachment.setPermission(name, value); return attachment; }
    default PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value, int ticks) { return addAttachment(plugin, name, value); }
    default void removeAttachment(PermissionAttachment attachment) { if (attachment != null) attachment.remove(); }
    default void recalculatePermissions() { }
    default java.util.Set<PermissionAttachmentInfo> getEffectivePermissions() { return java.util.Collections.emptySet(); }
}
