package foton;

import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import org.bukkit.permissions.Permissible;
import org.bukkit.permissions.Permission;
import org.bukkit.plugin.Plugin;

/** The server-wide permission declarations contributed by loaded plugins. */
public final class PermissionRegistry {
    private static final Map<String, List<Contribution>> contributions =
        new LinkedHashMap<>();
    private static final List<WeakReference<Permissible>> permissibles =
        new ArrayList<>();

    private PermissionRegistry() {}

    public static synchronized Permission get(String name) {
        Contribution selected = name == null ? null : selected(key(name));
        return selected == null ? null : selected.permission;
    }

    public static void add(Permission permission) {
        boolean changed;
        synchronized (PermissionRegistry.class) {
            changed = addContribution(null, permission);
        }
        if (changed) invalidate();
    }

    static void register(Plugin plugin, List<Permission> declared) {
        if (plugin == null || declared == null) return;
        boolean changed = false;
        synchronized (PermissionRegistry.class) {
            for (Permission permission : declared) {
                changed |= addContribution(plugin, permission);
            }
        }
        if (changed) invalidate();
    }

    private static boolean addContribution(Plugin owner, Permission permission) {
        if (permission == null || permission.getName().isEmpty()) return false;
        String name = key(permission.getName());
        List<Contribution> declarations =
            contributions.computeIfAbsent(name, ignored -> new ArrayList<>());
        declarations.removeIf(existing -> existing.owner == owner);
        declarations.add(new Contribution(owner, permission));
        return true;
    }

    /** Removes the currently selected declaration, revealing any later contributor. */
    public static void remove(String name) {
        if (name == null) return;
        boolean changed = false;
        synchronized (PermissionRegistry.class) {
            String normalized = key(name);
            List<Contribution> declarations = contributions.get(normalized);
            if (declarations != null && !declarations.isEmpty()) {
                declarations.remove(0);
                changed = true;
                if (declarations.isEmpty()) contributions.remove(normalized);
            }
        }
        if (changed) invalidate();
    }

    static void removeOwnedBy(Plugin plugin) {
        if (plugin == null) return;
        boolean changed = false;
        synchronized (PermissionRegistry.class) {
            var iterator = contributions.entrySet().iterator();
            while (iterator.hasNext()) {
                List<Contribution> declarations = iterator.next().getValue();
                changed |= declarations.removeIf(declaration -> declaration.owner == plugin);
                if (declarations.isEmpty()) iterator.remove();
            }
        }
        if (changed) invalidate();
    }

    /** Resolves descriptor defaults and parent-child grants for an operator state.
     *
     * @return a decision, or {@code null} when no plugin declared the node
     */
    public static synchronized Boolean resolveDefault(String name, boolean operator) {
        if (name == null) return null;
        String normalized = key(name);
        Map<String, Boolean> effective = effectiveDefaults(operator);
        Boolean inherited = effective.get(normalized);
        if (inherited != null) return inherited;
        Contribution direct = selected(normalized);
        return direct == null ? null : direct.permission.getDefault().getValue(operator);
    }

    /** Effective true defaults and every child decision they imply. */
    public static synchronized Map<String, Boolean> effectiveDefaults(boolean operator) {
        LinkedHashMap<String, Boolean> result = new LinkedHashMap<>();
        for (String name : contributions.keySet()) {
            Contribution selected = selected(name);
            if (selected != null && selected.permission.getDefault().getValue(operator)) {
                apply(result, selected.permission.getName(), true, new LinkedHashSet<>());
            }
        }
        return Collections.unmodifiableMap(result);
    }

    public static synchronized Map<String, Boolean> children(String name) {
        Contribution selected = name == null ? null : selected(key(name));
        return selected == null
            ? Map.of()
            : Collections.unmodifiableMap(new LinkedHashMap<>(selected.permission.getChildren()));
    }

    /** Tracks a Bukkit permissible so registry changes can invalidate its cached closure. */
    public static synchronized void track(Permissible permissible) {
        if (permissible == null) return;
        permissibles.removeIf(reference -> reference.get() == null);
        for (WeakReference<Permissible> reference : permissibles) {
            if (reference.get() == permissible) return;
        }
        permissibles.add(new WeakReference<>(permissible));
    }

    /** Recalculates every live permission cache after a descriptor mutation. */
    public static void changed() {
        invalidate();
    }

    private static void invalidate() {
        List<Permissible> live = new ArrayList<>();
        synchronized (PermissionRegistry.class) {
            var iterator = permissibles.iterator();
            while (iterator.hasNext()) {
                Permissible permissible = iterator.next().get();
                if (permissible == null) iterator.remove();
                else live.add(permissible);
            }
        }
        for (Permissible permissible : live) permissible.recalculatePermissions();
        FotonPlayer.recalculateAllPermissions();
    }

    private static void apply(Map<String, Boolean> result, String name, boolean value,
            Set<String> path) {
        String normalized = key(name);
        if (!path.add(normalized)) return;
        result.put(normalized, value);
        Contribution selected = selected(normalized);
        if (selected != null) {
            for (Map.Entry<String, Boolean> child
                    : selected.permission.getChildren().entrySet()) {
                apply(result, child.getKey(), value == child.getValue(), path);
            }
        }
        path.remove(normalized);
    }

    private static Contribution selected(String name) {
        List<Contribution> declarations = contributions.get(name);
        return declarations == null || declarations.isEmpty() ? null : declarations.get(0);
    }

    private static String key(String name) {
        return name.toLowerCase(Locale.ROOT);
    }

    private static final class Contribution {
        private final Plugin owner;
        private final Permission permission;

        private Contribution(Plugin owner, Permission permission) {
            this.owner = owner;
            this.permission = permission;
        }
    }
}
