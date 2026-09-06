package org.bukkit.permissions;

/** Object whose permission state can be queried. */
public interface Permissible extends ServerOperator {
    boolean hasPermission(String permission);

    /** Whether this is a server operator.
     *
     * <p>Defaulted rather than abstract because `Permissible` reaches every
     * entity through `CommandSender`, and a pig is not an operator. The types
     * that can be -- players, the console, offline players -- answer for
     * themselves and do so through Foton's own operator list. */
    @Override default boolean isOp() { return false; }

    @Override default void setOp(boolean value) { }

    /** Whether this permission has been decided, either way.
     *
     * <p>Not the same question as {@link #hasPermission(String)}: a node
     * explicitly set to {@code false} is set and not held, and plugins that
     * distinguish "denied" from "never mentioned" read this one. */
    default boolean isPermissionSet(String permission) { return hasPermission(permission); }

    default boolean isPermissionSet(Permission permission) {
        return permission != null && isPermissionSet(permission.getName());
    }
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
