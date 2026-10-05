import java.nio.file.Files;
import java.nio.file.Path;
import org.bukkit.plugin.java.JavaPlugin;

/** Foreign permission callbacks cannot strand a partially published plugin. */
public final class PermissionRollbackCheck {
    private PermissionRollbackCheck() {}

    public static void main(String[] args) throws Exception { check(); }

    static void check() throws Exception {
        checkFailure(false);
        checkFailure(true);
    }

    private static void checkFailure(boolean late) throws Exception {
        Path root = Files.createTempDirectory("foton-permission-rollback-");
        var permissible = new ThrowingPermissible();
        var healthy = new org.bukkit.permissions.PermissibleBase(null);
        var players = new PlayerCaches(healthy);
        var onLoadRan = new java.util.concurrent.atomic.AtomicBoolean();
        System.getProperties().put("foton.permission.rollback.arm", (Runnable) () -> {
            onLoadRan.set(true);
            Checks.expect(healthy.hasPermission("fixture.rollback"), "healthy default grant populated before onLoad failure");
            players.assertChild(true);
            permissible.armed = true;
        });
        try {
            Path source = root.resolve("PermissionFailure.java");
            Files.writeString(source, """
                public final class PermissionFailure extends org.bukkit.plugin.java.JavaPlugin {
                    public PermissionFailure() {
                        System.getProperties().put("foton.permission.rollback.instance", this);
                    }
                    public void onLoad() {
                        ((Runnable) System.getProperties().get("foton.permission.rollback.arm")).run();
                        throw new IllegalStateException("intentional onLoad failure after caches populate");
                    }
                }
                """);
            Path classes = Files.createDirectories(root.resolve("classes"));
            Checks.same(javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null,
                "--release", "21", "-classpath", System.getProperty("java.class.path"),
                "-d", classes.toString(), source.toString()), 0, "permission fixture compilation");
            try (var jar = new java.util.jar.JarOutputStream(Files.newOutputStream(root.resolve("failure.jar")))) {
                jar.putNextEntry(new java.util.jar.JarEntry("PermissionFailure.class"));
                Files.copy(classes.resolve("PermissionFailure.class"), jar);
                jar.closeEntry();
                jar.putNextEntry(new java.util.jar.JarEntry("permission-rollback-owned.marker"));
                jar.write(1);
                jar.closeEntry();
                jar.putNextEntry(new java.util.jar.JarEntry("plugin.yml"));
                jar.write(("name: PermissionFailure\nversion: 1\nmain: PermissionFailure\n"
                    + "provides: [PermissionAlias]\npermissions:\n  fixture.rollback:\n    default: true\n    children:\n      fixture.rollback.child: true\n")
                    .getBytes(java.nio.charset.StandardCharsets.UTF_8));
                jar.closeEntry();
            }
            permissible.armed = !late;
            Checks.same(foton.PluginHost.loadAllOnLoad(root.toString()), 0,
                "throwing permissible must roll back load");
            JavaPlugin instance = (JavaPlugin) System.getProperties().get("foton.permission.rollback.instance");
            Checks.expect(instance != null, "fixture must construct before permission failure");
            Checks.same(onLoadRan.get(), late, "publication failure must prevent onLoad; late failure must reach it");
            Checks.expect(permissible.failures >= (late ? 1 : 2), "rollback must survive cleanup callback failures");
            Checks.expect(!healthy.hasPermission("fixture.rollback"), "healthy default grant revoked despite first thrower");
            players.assertChild(false);
            Checks.expect(!instance.isEnabled(), "failed publication left enabled instance");
            Checks.expect(foton.PluginHost.byName("PermissionFailure") == null
                && foton.PluginHost.byName("PermissionAlias") == null,
                "failed publication retained real name or alias");
            Checks.expect(foton.PermissionRegistry.get("fixture.rollback") == null,
                "throwing cleanup callback retained permission contribution");
            Checks.expect(java.util.Arrays.stream(foton.PluginHost.all()).noneMatch(plugin -> plugin == instance),
                "failed publication retained plugin instance");
            try (var resource = instance.getClass().getClassLoader().getResourceAsStream("permission-rollback-owned.marker")) {
                Checks.expect(resource == null, "failed publication leaked loader resources");
            }
            try {
                org.bukkit.Bukkit.getScheduler().runTask(instance, () -> {});
                throw new AssertionError("failed publication retained scheduler admission");
            } catch (org.bukkit.plugin.IllegalPluginAccessException expected) { }
        } finally {
            permissible.armed = false;
            players.close();
            System.getProperties().remove("foton.permission.rollback.arm");
            Object instance = System.getProperties().remove("foton.permission.rollback.instance");
            if (instance instanceof JavaPlugin plugin) foton.PluginHost.cleanup(plugin);
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
    }

    /** Real player permission cache objects, without a networked player/native host. */
    private static final class PlayerCaches implements AutoCloseable {
        private final java.util.Map<?, ?> map;
        private final java.util.List<Object> keys = new java.util.ArrayList<>();
        private final java.util.List<Object> states = new java.util.ArrayList<>();
        private final java.lang.reflect.Field resolved;
        PlayerCaches(org.bukkit.permissions.Permissible permissible) throws Exception {
            var field = foton.FotonPlayer.class.getDeclaredField("PERMISSIONS");
            field.setAccessible(true);
            map = (java.util.Map<?, ?>) field.get(null);
            Class<?> stateType = Class.forName("foton.FotonPlayer$PlayerPermissions");
            var constructor = stateType.getDeclaredConstructor();
            constructor.setAccessible(true);
            resolved = stateType.getDeclaredField("resolved");
            resolved.setAccessible(true);
            var add = stateType.getDeclaredMethod("add", org.bukkit.permissions.PermissionAttachment.class);
            add.setAccessible(true);
            var session = Class.forName("foton.FotonPlayer$SessionKey").getDeclaredConstructor(java.util.UUID.class, long.class);
            session.setAccessible(true);
            var owner = new JavaPlugin() {};
            for (int index = 0; index < 2; index++) {
                Object state = constructor.newInstance();
                var attachment = new org.bukkit.permissions.PermissionAttachment(owner, permissible);
                attachment.setPermission("fixture.rollback", true);
                add.invoke(state, attachment);
                Object key = session.newInstance(java.util.UUID.randomUUID(), 1L);
                java.util.Map.class.getMethod("put", Object.class, Object.class).invoke(map, key, state);
                keys.add(key);
                states.add(state);
            }
        }
        void assertChild(boolean present) {
            try {
                for (Object state : states) Checks.expect(
                    ((java.util.Map<?, ?>) resolved.get(state)).containsKey("fixture.rollback.child") == present,
                    "every player cache must invalidate despite foreign permissible failure");
            } catch (IllegalAccessException error) { throw new AssertionError(error); }
        }
        public void close() { for (Object key : keys) map.remove(key); }
    }

    private static final class ThrowingPermissible extends org.bukkit.permissions.PermissibleBase {
        boolean armed;
        int failures;
        ThrowingPermissible() { super(null); }
        @Override public void recalculatePermissions() {
            if (armed) {
                failures++;
                throw new IllegalStateException("intentional permissible callback failure");
            }
            super.recalculatePermissions();
        }
    }
}
