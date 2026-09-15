/** Everything CI checks about the plugin API, from one place.
 *
 * These are real Java files rather than a heredoc in the build script because
 * they have grown past what a shell script should be carrying, and because a
 * check that is hard to read is a check nobody fixes when it breaks.
 */
public final class Checks {
    public static void main(String[] args) throws Exception {
        Services.check();
        Events.check(args[0]);
        Config.check();
        Geometry.check();
        InventoryViewCheck.check();
        Items.check();
        Colors.check();
        Commands.check();
        foton.PluginHost.disableAll();
        Checks.expect(foton.CommandMap.get("fixture") == null,
            "disabling a plugin should release the names it claimed");
        Checks.expect(org.bukkit.Bukkit.getMessenger().getIncomingChannels().isEmpty()
            && org.bukkit.Bukkit.getMessenger().getOutgoingChannels().isEmpty(),
            "disabling a plugin should release its custom channels");
        lifecycle();
        YamlCheck.check();
        System.out.println(
            "plugin API checked: services, events, scheduler, YAML, configuration,\n"
                + "    geometry, items, colors and commands");
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

    private static void lifecycle() throws Exception {
        java.nio.file.Path directory = java.nio.file.Files.createTempDirectory(
            "foton-lifecycle-fixtures-");
        try {
            fixture(directory, "FailingEnable", "example.FailingEnablePlugin",
                "commands:\n  failingenable: {}\n");
            fixture(directory, "EnableDependent", "example.EnableDependentPlugin",
                "depend: [FailingEnable]\n");
            fixture(directory, "FailingLoad", "example.FailingLoadPlugin",
                "commands:\n  failingload: {}\n");
            fixture(directory, "LoadDependent", "example.LoadDependentPlugin",
                "depend: [FailingLoad]\n");
            fixture(directory, "Healthy", "example.HealthyPlugin", "");

            int enabled;
            try {
                enabled = foton.PluginHost.loadAll(directory.toString());
            } catch (UnsatisfiedLinkError cleanupWasNotIsolated) {
                enabled = -1;
            }
            same(enabled, 1,
                "only the unrelated lifecycle fixture should enable");
            same(example.FailingLoadPlugin.disableCalls, 0,
                "onDisable ran even though enable was never attempted");
            expect(example.FailingLoadPlugin.visibleDuringLoad,
                "plugin was not registered before onLoad");
            expect(example.FailingEnablePlugin.disabledSawFalse,
                "onDisable observed enabled=true");
            same(example.FailingEnablePlugin.disableCalls, 1,
                "failed enable should invoke onDisable exactly once");
            same(example.LoadDependentPlugin.loadCalls, 0,
                "dependent onLoad ran after its required dependency failed onLoad");
            same(example.LoadDependentPlugin.enableCalls, 0,
                "dependent onEnable ran after its required dependency failed onLoad");
            same(example.EnableDependentPlugin.loadCalls, 1,
                "enable-phase dependent should load before its dependency fails to enable");
            same(example.EnableDependentPlugin.enableCalls, 0,
                "dependent onEnable ran after its required dependency failed onEnable");
            same(example.HealthyPlugin.loadCalls, 1,
                "unrelated plugin did not continue through onLoad");
            same(example.HealthyPlugin.enableCalls, 1,
                "unrelated plugin did not continue through onEnable");

            same(foton.EventBridge.handlerCount("example.FailingEvent"), 0,
                "failed plugin listener leaked");
            same(org.bukkit.Bukkit.getServer().getPluginCommand("failingload"), null,
                "failed onLoad command leaked");
            same(org.bukkit.Bukkit.getServer().getPluginCommand("failingenable"), null,
                "failed onEnable command leaked");
            same(pendingTasks("FailingLoad"), 0, "failed onLoad task leaked");
            same(pendingTasks("FailingEnable"), 0, "failed onEnable task leaked");
            resourcesReleased(example.FailingLoadPlugin.instance, "failed onLoad");
            resourcesReleased(example.FailingEnablePlugin.instance, "failed onEnable");
            expect(foton.PluginHost.byName("FailingLoad") == null
                    && foton.PluginHost.byName("FailingEnable") == null
                    && foton.PluginHost.byName("LoadDependent") == null
                    && foton.PluginHost.byName("EnableDependent") == null,
                "failed plugin or required dependent remained registered");

            cleanupTwice(example.FailingEnablePlugin.instance);
            same(example.FailingEnablePlugin.disableCalls, 1,
                "transactional cleanup was not idempotent");
        } finally {
            foton.PluginHost.disableAll();
            deleteTree(directory);
        }
    }

    private static void fixture(
            java.nio.file.Path directory, String name, String main, String extra)
            throws Exception {
        java.nio.file.Path jar = directory.resolve(name + ".jar");
        try (java.util.jar.JarOutputStream output = new java.util.jar.JarOutputStream(
                java.nio.file.Files.newOutputStream(jar))) {
            output.putNextEntry(new java.util.jar.JarEntry("plugin.yml"));
            output.write(("name: " + name + "\nversion: 1\nmain: " + main + "\n" + extra)
                .getBytes(java.nio.charset.StandardCharsets.UTF_8));
            output.closeEntry();
        }
    }

    private static int pendingTasks(String owner) {
        int count = 0;
        for (org.bukkit.scheduler.BukkitTask task
                : org.bukkit.Bukkit.getScheduler().getPendingTasks()) {
            if (task.getOwner().getName().equals(owner)) count++;
        }
        return count;
    }

    private static void resourcesReleased(org.bukkit.plugin.Plugin plugin, String phase) {
        expect(plugin != null, phase + " fixture did not retain its instance");
        expect(org.bukkit.Bukkit.getServicesManager().getRegistrations(plugin).isEmpty(),
            phase + " service leaked");
        expect(org.bukkit.Bukkit.getMessenger().getIncomingChannels(plugin).isEmpty()
                && org.bukkit.Bukkit.getMessenger().getOutgoingChannels(plugin).isEmpty(),
            phase + " channel leaked");
    }

    private static void cleanupTwice(org.bukkit.plugin.Plugin plugin) {
        foton.PluginHost.cleanup(plugin);
        foton.PluginHost.cleanup(plugin);
    }

    private static void deleteTree(java.nio.file.Path directory) throws Exception {
        try (java.util.stream.Stream<java.nio.file.Path> paths = java.nio.file.Files.walk(directory)) {
            for (java.nio.file.Path path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) {
                java.nio.file.Files.deleteIfExists(path);
            }
        }
    }
}
