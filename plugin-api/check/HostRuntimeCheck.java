import fixture.jdbc.HostDriver;

/** JVM-isolated checks for process-global JDBC and plugin lifecycle state. */
public final class HostRuntimeCheck {
    private HostRuntimeCheck() {}

    public static void main(String[] args) throws Exception {
        switch (args[0]) {
            case "jdbc" -> jdbc(args[1]);
            case "self-disable" -> selfDisable(args[1]);
            default -> throw new IllegalArgumentException(args[0]);
        }
    }

    private static void jdbc(String directory) {
        System.clearProperty("foton.fixture.privateDriverRegistrations");
        ClassLoader original = Thread.currentThread().getContextClassLoader();
        ClassLoader blind = new ClassLoader(null) {};
        Thread.currentThread().setContextClassLoader(blind);
        try {
            Checks.same(foton.PluginHost.loadAll(directory), 1,
                "host-classpath JDBC fixture enabled count");
            Checks.same(Thread.currentThread().getContextClassLoader(), blind,
                "plugin loading changed the thread context classloader");
            Checks.same(HostDriver.connections(), 1,
                "host JDBC provider did not open the first connection");
            Checks.same(HostDriver.registeredDrivers(), 1L,
                "host JDBC provider registration count after first load");
            Checks.same(System.getProperty("foton.fixture.privateDriverRegistrations"), null,
                "plugin-private JDBC provider was process-globally initialized");

            foton.PluginHost.disableAll();
            Checks.same(foton.PluginHost.loadAll(directory), 1,
                "host-classpath JDBC fixture reload enabled count");
            Checks.same(HostDriver.connections(), 2,
                "host JDBC provider did not open the reload connection");
            Checks.same(HostDriver.registeredDrivers(), 1L,
                "reload accumulated host JDBC registrations");
            Checks.same(System.getProperty("foton.fixture.privateDriverRegistrations"), null,
                "reload initialized a plugin-private JDBC provider");
        } finally {
            foton.PluginHost.disableAll();
            Thread.currentThread().setContextClassLoader(original);
        }
    }

    private static void selfDisable(String directory) throws Exception {
        System.clearProperty("foton.fixture.selfDisableEnableCalls");
        System.clearProperty("foton.fixture.selfDisableDisableCalls");
        System.clearProperty("foton.fixture.selfDisableEnableEvents");
        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
        java.io.PrintStream original = System.out;
        int enabled;
        try (java.io.PrintStream capture = new java.io.PrintStream(
                bytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            enabled = foton.PluginHost.loadAll(directory);
        } finally {
            System.setOut(original);
        }
        try {
            String log = bytes.toString(java.nio.charset.StandardCharsets.UTF_8);
            Checks.same(enabled, 1, "only the observer should remain enabled");
            Checks.same(System.getProperty("foton.fixture.selfDisableEnableCalls"), "1",
                "self-disabling plugin onEnable call count");
            Checks.same(System.getProperty("foton.fixture.selfDisableDisableCalls"), "1",
                "self-disabling plugin onDisable call count");
            Checks.same(System.getProperty("foton.fixture.selfDisableEnableEvents"), null,
                "self-disabling plugin received PluginEnableEvent");
            Checks.expect(!log.contains("[host] enabled SelfDisabling v1"),
                "self-disabling plugin logged a successful enable: " + log);
            Checks.expect(foton.PluginHost.byName("SelfDisabling") == null,
                "self-disabling plugin remained registered");
        } finally {
            foton.PluginHost.disableAll();
        }
    }
}
