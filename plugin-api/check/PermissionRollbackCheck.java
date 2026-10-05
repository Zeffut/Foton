import java.nio.file.Files;
import java.nio.file.Path;
import org.bukkit.plugin.java.JavaPlugin;

/** Foreign permission callbacks cannot strand a partially published plugin. */
public final class PermissionRollbackCheck {
    private PermissionRollbackCheck() {}

    public static void main(String[] args) throws Exception { check(); }

    static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-permission-rollback-");
        var permissible = new ThrowingPermissible();
        try {
            Path source = root.resolve("PermissionFailure.java");
            Files.writeString(source, """
                public final class PermissionFailure extends org.bukkit.plugin.java.JavaPlugin {
                    public PermissionFailure() {
                        System.getProperties().put("foton.permission.rollback.instance", this);
                    }
                    public void onLoad() {
                        throw new AssertionError("onLoad ran after permission publication failed");
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
                    + "provides: [PermissionAlias]\npermissions:\n  fixture.rollback: {}\n")
                    .getBytes(java.nio.charset.StandardCharsets.UTF_8));
                jar.closeEntry();
            }
            permissible.armed = true;
            Checks.same(foton.PluginHost.loadAllOnLoad(root.toString()), 0,
                "throwing permissible must roll back load");
            JavaPlugin instance = (JavaPlugin) System.getProperties().get("foton.permission.rollback.instance");
            Checks.expect(instance != null, "fixture must construct before permission failure");
            Checks.expect(permissible.failures >= 2, "rollback must survive a second cleanup callback failure");
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
            Object instance = System.getProperties().remove("foton.permission.rollback.instance");
            if (instance instanceof JavaPlugin plugin) foton.PluginHost.cleanup(plugin);
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
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
