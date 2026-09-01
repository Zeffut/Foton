/** Everything CI checks about the plugin API, from one place.
 *
 * These are real Java files rather than a heredoc in the build script because
 * they have grown past what a shell script should be carrying, and because a
 * check that is hard to read is a check nobody fixes when it breaks.
 */
public final class Checks {
    public static void main(String[] args) throws Exception {
        Events.check(args[0]);
        Config.check();
        Geometry.check();
        Commands.check();
        foton.PluginHost.disableAll();
        Checks.expect(foton.CommandMap.get("fixture") == null,
            "disabling a plugin should release the names it claimed");
        YamlCheck.check();
        System.out.println(
            "plugin API checked: events, scheduler, YAML, configuration, geometry and commands");
    }

    static void expect(boolean condition, String what) {
        if (!condition) {
            throw new AssertionError(what);
        }
    }

    static void same(Object actual, Object expected, String what) {
        if (expected == null ? actual != null : !expected.equals(actual)) {
            throw new AssertionError(what + ": expected " + expected + ", got " + actual);
        }
    }
}
