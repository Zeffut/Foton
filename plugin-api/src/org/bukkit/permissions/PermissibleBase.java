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
        foton.PermissionRegistry.track(this);
    }

    @Override public boolean isOp() { return opable != null && opable.isOp(); }

    @Override public void setOp(boolean value) {
        if (opable == null) {
            throw new UnsupportedOperationException("Cannot change op value as no ServerOperator is set");
        }
        opable.setOp(value);
        recalculatePermissions();
    }

    @Override public boolean isPermissionSet(String permission) {
        return permission != null && permissions.containsKey(permission.toLowerCase(java.util.Locale.ROOT));
    }

    @Override public boolean hasPermission(String permission) {
        if (permission == null) return false;
        PermissionAttachmentInfo info = permissions.get(permission.toLowerCase(java.util.Locale.ROOT));
        if (info != null) return info.getValue();
        Boolean declared = foton.PermissionRegistry.resolveDefault(permission, isOp());
        // An undeclared node falls back to op-ness, which is Bukkit's default
        // for a permission no plugin registered.
        return declared == null ? isOp() : declared;
    }

    @Override public PermissionAttachment addAttachment(Plugin plugin) {
        PermissionAttachment attachment = new PermissionAttachment(plugin, this);
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
        recalculatePermissions();
    }

    @Override public void recalculatePermissions() {
        permissions.clear();
        for (Map.Entry<String, Boolean> entry
                : foton.PermissionRegistry.effectiveDefaults(isOp()).entrySet()) {
            permissions.put(entry.getKey(), new PermissionAttachmentInfo(
                this, entry.getKey(), null, entry.getValue()));
        }
        for (PermissionAttachment attachment : attachments) {
            for (Map.Entry<String, Boolean> entry : attachment.getPermissions().entrySet()) {
                apply(attachment, entry.getKey(), entry.getValue(), new LinkedHashSet<>());
            }
        }
    }

    private void apply(PermissionAttachment attachment, String permission, boolean value,
            Set<String> path) {
        String node = permission.toLowerCase(java.util.Locale.ROOT);
        if (!path.add(node)) return;
        permissions.put(node, new PermissionAttachmentInfo(this, node, attachment, value));
        for (Map.Entry<String, Boolean> child
                : foton.PermissionRegistry.children(node).entrySet()) {
            apply(attachment, child.getKey(), value == child.getValue(), path);
        }
        path.remove(node);
    }

    @Override public Set<PermissionAttachmentInfo> getEffectivePermissions() {
        return new LinkedHashSet<>(permissions.values());
    }
}
