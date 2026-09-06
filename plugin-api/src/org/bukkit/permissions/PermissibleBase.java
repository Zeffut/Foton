package org.bukkit.permissions;

import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.Map;
import java.util.Set;
import org.bukkit.plugin.Plugin;

/** The permission bookkeeping Bukkit gives anything that can hold permissions.
 *
 * <p>Plugins construct one directly when they wrap a command sender of their
 * own -- a fake console, a proxy player, a test double -- and delegate their
 * `Permissible` methods to it. That is the whole reason it is public API rather
 * than an implementation detail, and it is why the {@link ServerOperator}
 * constructor matters: the wrapped object decides op-ness, and this decides
 * everything else.
 */
public class PermissibleBase implements Permissible {
    private final ServerOperator opable;
    private final Map<String, PermissionAttachmentInfo> permissions = new LinkedHashMap<>();
    private final Set<PermissionAttachment> attachments = new LinkedHashSet<>();

    public PermissibleBase(ServerOperator opable) {
        this.opable = opable;
        recalculatePermissions();
    }

    @Override public boolean isOp() { return opable != null && opable.isOp(); }

    @Override public void setOp(boolean value) {
        if (opable == null) {
            throw new UnsupportedOperationException("Cannot change op value as no ServerOperator is set");
        }
        opable.setOp(value);
    }

    @Override public boolean isPermissionSet(String permission) {
        return permission != null && permissions.containsKey(permission.toLowerCase(java.util.Locale.ROOT));
    }

    @Override public boolean hasPermission(String permission) {
        if (permission == null) return false;
        PermissionAttachmentInfo info = permissions.get(permission.toLowerCase(java.util.Locale.ROOT));
        // An unset node falls back to op-ness, which is Bukkit's default for a
        // permission no plugin has registered. Returning false instead would
        // lock operators out of commands that never declared a node.
        return info == null ? isOp() : info.getValue();
    }

    @Override public PermissionAttachment addAttachment(Plugin plugin) {
        PermissionAttachment attachment = new PermissionAttachment(plugin);
        attachments.add(attachment);
        recalculatePermissions();
        return attachment;
    }

    @Override public PermissionAttachment addAttachment(Plugin plugin, String name, boolean value) {
        PermissionAttachment attachment = addAttachment(plugin);
        attachment.setPermission(name, value);
        recalculatePermissions();
        return attachment;
    }

    @Override public void removeAttachment(PermissionAttachment attachment) {
        if (attachment == null) return;
        attachments.remove(attachment);
        attachment.remove();
        recalculatePermissions();
    }

    @Override public void recalculatePermissions() {
        permissions.clear();
        for (PermissionAttachment attachment : attachments) {
            for (Map.Entry<String, Boolean> entry : attachment.getPermissions().entrySet()) {
                String node = entry.getKey().toLowerCase(java.util.Locale.ROOT);
                permissions.put(node, new PermissionAttachmentInfo(this, node, attachment, entry.getValue()));
            }
        }
    }

    @Override public Set<PermissionAttachmentInfo> getEffectivePermissions() {
        return new LinkedHashSet<>(permissions.values());
    }
}
