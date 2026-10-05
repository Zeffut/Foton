package foton;

import java.nio.file.Files;
import java.nio.file.Path;

/** Compile against the immutable C interface, execute that caller against today's API. */
final class LegacyLoggerAbiCheck {
    private LegacyLoggerAbiCheck() {}

    static void check() throws Exception {
        Path root = Files.createTempDirectory("foton-old-logger-abi-");
        try {
            Path caller = root.resolve("OldLoggerCaller.java");
            Files.writeString(caller, """
                public final class OldLoggerCaller {
                    public static org.slf4j.Logger read(io.papermc.paper.plugin.bootstrap.PluginProviderContext context) {
                        return context.getLogger();
                    }
                }
                """);
            Path classes = Files.createDirectory(root.resolve("classes"));
            int result = javax.tools.ToolProvider.getSystemJavaCompiler().run(null, null, null,
                "--release", "21", "-cp", System.getProperty("java.class.path"), "-d", classes.toString(),
                "plugin-api/check/legacy-provider-context/PluginProviderContext.java", caller.toString());
            if (result != 0) throw new AssertionError("old interface/caller compilation failed");
            // Parent first: the old interface is NOT loaded. Only its compiled caller is reused.
            try (var loader = new java.net.URLClassLoader(new java.net.URL[] {classes.toUri().toURL()},
                    LegacyLoggerAbiCheck.class.getClassLoader())) {
                var type = io.papermc.paper.plugin.bootstrap.PluginProviderContext.class;
                if (loader.loadClass(type.getName()) != type) throw new AssertionError("wrong runtime interface");
                var logger = net.kyori.adventure.text.logger.slf4j.ComponentLogger.logger("old-abi");
                var context = new io.papermc.paper.plugin.bootstrap.PluginProviderContext() {
                    public io.papermc.paper.plugin.configuration.PluginMeta getConfiguration() { return null; }
                    public Path getDataDirectory() { return root; }
                    public Path getPluginSource() { return root; }
                    public net.kyori.adventure.text.logger.slf4j.ComponentLogger getLogger() { return logger; }
                };
                Object actual = loader.loadClass("OldLoggerCaller").getMethod("read", type).invoke(null, context);
                if (actual != logger) throw new AssertionError("old Logger descriptor did not reach covariant implementation");
            }
        } finally {
            try (var paths = Files.walk(root)) {
                for (Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
    }
}
