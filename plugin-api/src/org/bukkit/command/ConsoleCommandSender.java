package org.bukkit.command;

/** The server console as a command source. */
public interface ConsoleCommandSender extends CommandSender, org.bukkit.permissions.ServerOperator {
    @Override boolean isOp();
    default net.kyori.adventure.text.Component name() {
        return net.kyori.adventure.text.Component.text(getName());
    }
    default org.bukkit.permissions.PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin) { return new org.bukkit.permissions.PermissionAttachment(plugin); }
    default org.bukkit.permissions.PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, int ticks) { return addAttachment(plugin); }
    default org.bukkit.permissions.PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value) { org.bukkit.permissions.PermissionAttachment a = addAttachment(plugin); a.setPermission(name, value); return a; }
    default org.bukkit.permissions.PermissionAttachment addAttachment(org.bukkit.plugin.Plugin plugin, String name, boolean value, int ticks) { return addAttachment(plugin, name, value); }
    default void removeAttachment(org.bukkit.permissions.PermissionAttachment attachment) { if (attachment != null) attachment.remove(); }
    default java.util.Set<org.bukkit.permissions.PermissionAttachmentInfo> getEffectivePermissions() { return java.util.Collections.emptySet(); }
    default boolean isPermissionSet(org.bukkit.permissions.Permission permission) { return permission != null && isPermissionSet(permission.getName()); }
    default void recalculatePermissions() { }
    default void setOp(boolean value) { }
    default void sendMessage(net.kyori.adventure.text.ComponentLike message) {
        if (message != null) sendMessage(message.asComponent());
    }
}
