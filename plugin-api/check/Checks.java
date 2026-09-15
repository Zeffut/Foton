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
        lifecycle(args[1], args[2]);
        dependencies(args[3], args[4], args[5], args[6]);
        libraries(args[7], args[8], args[9], args[10]);
        paper(args[11], args[12]);
        YamlCheck.check();
        assertHostIsEmpty();
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

    private static void assertHostIsEmpty() throws Exception {
        same(foton.PluginHost.all().length, 0,
            "disableAll retained plugins");
        java.lang.reflect.Field handlers = foton.EventBridge.class
            .getDeclaredField("handlers");
        handlers.setAccessible(true);
        int handlerCount = ((java.util.Map<?, ?>) handlers.get(null)).values().stream()
            .mapToInt(value -> ((java.util.List<?>) value).size()).sum();
        same(handlerCount, 0, "disableAll retained event handlers");
        same(foton.CommandMap.knownCommands().size(), 0,
            "disableAll retained commands");
        same(org.bukkit.Bukkit.getScheduler().getPendingTasks().size(), 0,
            "disableAll retained pending tasks");
    }

    private static void lifecycle(String directory, String replacementDirectory)
            throws Exception {
        fixture.LifecycleProbe.reset();
        int enabled;
        int replacementEnabled;
        try {
            enabled = foton.PluginHost.loadAll(directory);
            replacementEnabled = foton.PluginHost.loadAll(replacementDirectory);
        } finally {
            fixture.LifecycleProbe.releaseAsync();
            fixture.LifecycleProbe.releaseSync();
        }
        fixture.LifecycleProbe.awaitAsyncFinished();
        fixture.LifecycleProbe.awaitSyncFinished();
        awaitSchedulerTasks("FailingEnable", 1);
        boolean retainedAsyncCancelled =
            fixture.LifecycleProbe.takeRetainedAsyncCancelled();
        int queuedAfterCleanup =
            foton.LifecycleDiagnostics.queuedTasks("FailingEnable");
        int syncRunsAfterCleanup =
            fixture.LifecycleProbe.calls("FailingEnable.syncRun");
        foton.FotonScheduler.tick();
        try {
            same(enabled, 1,
                "only the unrelated lifecycle fixture should enable");
            same(replacementEnabled, 1,
                "same-name replacement generation should enable");
            same(fixture.LifecycleProbe.calls("FailingLoad.disable"), 0,
                "onDisable ran even though enable was never attempted");
            expect(fixture.LifecycleProbe.flag("FailingLoad.visible"),
                "plugin was not registered before onLoad");
            expect(fixture.LifecycleProbe.flag("FailingEnable.disabledSawFalse"),
                "onDisable observed enabled=true");
            same(fixture.LifecycleProbe.calls("FailingEnable.disable"), 1,
                "failed enable should invoke onDisable exactly once");
            same(fixture.LifecycleProbe.calls("LoadDependent.load"), 0,
                "dependent onLoad ran after its required dependency failed onLoad");
            same(fixture.LifecycleProbe.calls("LoadDependent.enable"), 0,
                "dependent onEnable ran after its required dependency failed onLoad");
            same(fixture.LifecycleProbe.calls("EnableDependent.load"), 1,
                "enable-phase dependent should load before its dependency fails to enable");
            same(fixture.LifecycleProbe.calls("EnableDependent.enable"), 0,
                "dependent onEnable ran after its required dependency failed onEnable");
            same(fixture.LifecycleProbe.calls("Healthy.load"), 1,
                "unrelated plugin did not continue through onLoad");
            same(fixture.LifecycleProbe.calls("Healthy.enable"), 1,
                "unrelated plugin did not continue through onEnable");
            expect(fixture.LifecycleProbe.flag("FailingLoad.loader")
                    && fixture.LifecycleProbe.flag("FailingLoad.eventLoader")
                    && fixture.LifecycleProbe.flag("FailingLoad.lambdaLoader")
                    && fixture.LifecycleProbe.flag("FailingEnable.loader")
                    && fixture.LifecycleProbe.flag("EnableDependent.loader")
                    && fixture.LifecycleProbe.flag("Healthy.loader"),
                "lifecycle fixture class, event, or lambda came from the parent classpath");
            expect(fixture.LifecycleProbe.flag("Replacement.loader"),
                "replacement fixture class came from the parent classpath");
            same(fixture.LifecycleProbe.calls("Replacement.load"), 1,
                "same-name replacement did not load");
            same(fixture.LifecycleProbe.calls("Replacement.enable"), 1,
                "same-name replacement did not enable");
            same(foton.LifecycleDiagnostics.schedulerGenerations("FailingEnable"), 1,
                "scheduler retained the failed same-name generation after reload");

            same(fixture.LifecycleProbe.calls("FailingEnable.oldCancelTasks"), 1,
                "held old generation did not exercise cancelTasks after reload");
            same(fixture.LifecycleProbe.followUpAttempts(), 2,
                "running async rollback task did not attempt both follow-ups");
            same(fixture.LifecycleProbe.followUpRejections(), 2,
                "running async rollback task scheduled work after cleanup began");
            same(fixture.LifecycleProbe.followUpRuns(), 0,
                "a post-cleanup follow-up task ran");
            expect(retainedAsyncCancelled,
                "cancelled-running task handle lost terminal cancellation state");
            expect(!fixture.LifecycleProbe.flag("scheduler.gameTickBlocked"),
                "a game-tick scheduler API blocked on the lifecycle monitor");
            expect(fixture.LifecycleProbe.flag("FailingEnable.removedFromQueue"),
                "sync fixture did not capture a task in the tick-local keep state");
            same(syncRunsAfterCleanup, 1,
                "sync task did not finish its one cleanup-racing invocation");
            same(fixture.LifecycleProbe.calls("FailingEnable.syncRun"), 1,
                "cancelled repeating sync work ran again after cleanup");
            same(fixture.LifecycleProbe.calls("FailingEnable.keptRun"), 0,
                "tick-local sync work ran after cleanup");
            same(queuedAfterCleanup, 1,
                "pending queue retained work beyond the replacement's one task");
            same(fixture.LifecycleProbe.calls("Replacement.taskRun"), 1,
                "old generation cancellation removed the replacement task");

            same(foton.EventBridge.handlerCount("example.FailingEvent"), 0,
                "failed plugin listener leaked");
            same(org.bukkit.Bukkit.getServer().getPluginCommand("failingload"), null,
                "failed onLoad command leaked");
            same(org.bukkit.Bukkit.getServer().getPluginCommand("failingenable"), null,
                "failed onEnable command leaked");
            same(pendingTasks("FailingLoad"), 0, "failed onLoad task leaked");
            same(pendingTasks("FailingEnable"), 0, "failed onEnable task leaked");
            expect(org.bukkit.Bukkit.getServicesManager().getKnownServices().isEmpty(),
                "failed plugin service leaked");
            expect(org.bukkit.Bukkit.getMessenger().getIncomingChannels().isEmpty()
                    && org.bukkit.Bukkit.getMessenger().getOutgoingChannels().isEmpty(),
                "failed plugin channel leaked");
            expect(foton.PluginHost.byName("FailingLoad") == null
                    && foton.PluginHost.byName("LoadDependent") == null
                    && foton.PluginHost.byName("EnableDependent") == null,
                "failed plugin or required dependent remained registered");
            for (String failed : java.util.List.of(
                    "FailingLoad", "LoadDependent", "EnableDependent")) {
                same(foton.LifecycleDiagnostics.schedulerTasks(failed), 0,
                    "scheduler retained " + failed + " work");
                expect(!foton.LifecycleDiagnostics.schedulerAccepts(failed),
                    "scheduler retained " + failed + " submission rights");
                same(foton.LifecycleDiagnostics.hostReferences(failed), 0,
                    "host retained " + failed + " plugin or classloader references");
            }
            same(foton.LifecycleDiagnostics.eventTypes("example.FailingEvent"), 0,
                "event bridge retained the plugin-defined event class");

            foton.PluginHost.disableAll();
            same(fixture.LifecycleProbe.calls("Replacement.disable"), 1,
                "same-name replacement did not disable exactly once");
            expect(foton.PluginHost.byName("FailingEnable") == null,
                "replacement plugin remained registered after disable");
            same(foton.LifecycleDiagnostics.schedulerTasks("FailingEnable"), 0,
                "scheduler retained same-name plugin work after disable");
            expect(!foton.LifecycleDiagnostics.schedulerAccepts("FailingEnable"),
                "scheduler retained same-name submission rights after disable");
            same(foton.LifecycleDiagnostics.schedulerGenerations("FailingEnable"), 0,
                "scheduler retained an old or replacement generation after disable");
            same(foton.LifecycleDiagnostics.hostReferences("FailingEnable"), 0,
                "host retained old or replacement same-name generation");
            same(foton.LifecycleDiagnostics.hostReferences("Healthy"), 0,
                "host retained the healthy plugin after disable");
        } finally {
            foton.PluginHost.disableAll();
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

    private static void dependencies(
            String graphDirectory, String duplicateDirectory,
            String existingDirectory, String crossCallDirectory)
            throws Exception {
        clearDependencyProperties();
        java.io.ByteArrayOutputStream graphBytes = new java.io.ByteArrayOutputStream();
        java.io.PrintStream original = System.out;
        int enabled;
        try (java.io.PrintStream capture = new java.io.PrintStream(
                graphBytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            enabled = foton.PluginHost.loadAll(graphDirectory);
        } finally {
            System.setOut(original);
        }
        String graphLog = graphBytes.toString(java.nio.charset.StandardCharsets.UTF_8);
        try {
            same(enabled, 16, "dependency graph enabled count; load="
                + csvProperty("foton.fixture.dependencies.load") + "; log=" + graphLog);
            same(csvProperty("foton.fixture.dependencies.load"), java.util.List.of(
                "AfterConsumer", "AfterProvider", "AOmitConsumer", "BootstrapProvider",
                "NoJoinProvider", "NoJoinConsumer", "OptionalA", "OptionalB",
                "PaperConsumer", "Provider", "Consumer", "ServerAfter", "Unrelated",
                "ZOmitProvider", "ZuluBefore", "AlphaTarget"),
                "deterministic dependency load order");
            same(csvProperty("foton.fixture.dependencies.enable"), csvProperty(
                "foton.fixture.dependencies.load"),
                "dependency enable order should match load order");
            same(System.getProperty("foton.fixture.dependencies.alias"), "Provider",
                "consumer dependency alias lookup");
            same(System.getProperty("foton.fixture.dependencies.joined"), "true",
                "join-classpath dependency visibility");
            same(System.getProperty("foton.fixture.dependencies.isolated"), "true",
                "join-classpath false dependency isolation");
            same(System.getProperty("foton.fixture.dependencies.afterJoined"), "true",
                "AFTER dependency classpath visibility");
            same(System.getProperty("foton.fixture.dependencies.omitJoined"), "true",
                "consumer-first OMIT dependency classpath visibility");
            expect(foton.PluginHost.byName("VaUlT") != null
                    && foton.PluginHost.byName("VaUlT").getName().equals("Provider"),
                "case-insensitive provided alias lookup");
            expect(graphLog.contains("optional dependency cycle")
                    && graphLog.contains("OptionalA") && graphLog.contains("OptionalB"),
                "optional cycle was not broken with a named diagnostic: " + graphLog);
            expect(graphLog.contains("required dependency cycle")
                    && graphLog.contains("RequiredCycleA")
                    && graphLog.contains("RequiredCycleB"),
                "required cycle diagnostic did not name every member: " + graphLog);
            expect(!csvProperty("foton.fixture.dependencies.load").contains("BrokenRequired")
                    && !csvProperty("foton.fixture.dependencies.load")
                        .contains("BrokenDependent"),
                "required dependency failure did not propagate");
            expect(csvProperty("foton.fixture.dependencies.load").contains("Unrelated"),
                "unrelated plugin did not continue after dependency failures");
        } finally {
            foton.PluginHost.disableAll();
        }

        clearDependencyProperties();
        java.io.ByteArrayOutputStream duplicateBytes = new java.io.ByteArrayOutputStream();
        try (java.io.PrintStream capture = new java.io.PrintStream(
                duplicateBytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            enabled = foton.PluginHost.loadAll(duplicateDirectory);
        } finally {
            System.setOut(original);
        }
        String duplicateLog = duplicateBytes.toString(java.nio.charset.StandardCharsets.UTF_8);
        try {
            same(enabled, 1, "duplicate descriptor isolation");
            same(csvProperty("foton.fixture.dependencies.load"),
                java.util.List.of("HealthyDuplicate"),
                "duplicate names or aliases should reject every conflicting provider");
            expect(duplicateLog.contains("duplicate alias")
                    && duplicateLog.contains("AliasOne")
                    && duplicateLog.contains("AliasTwo"),
                "duplicate alias diagnostic did not name its providers: " + duplicateLog);
            expect(duplicateLog.contains("duplicate plugin name")
                    && duplicateLog.contains("SameName")
                    && duplicateLog.contains("samename"),
                "duplicate real-name diagnostic did not name its plugins: " + duplicateLog);
        } finally {
            foton.PluginHost.disableAll();
            clearDependencyProperties();
        }

        clearDependencyProperties();
        int existingEnabled = foton.PluginHost.loadAll(existingDirectory);
        org.bukkit.plugin.Plugin existing = foton.PluginHost.byName("ExistingProvider");
        java.io.ByteArrayOutputStream crossCallBytes = new java.io.ByteArrayOutputStream();
        int crossCallEnabled;
        try (java.io.PrintStream capture = new java.io.PrintStream(
                crossCallBytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            crossCallEnabled = foton.PluginHost.loadAll(crossCallDirectory);
        } finally {
            System.setOut(original);
        }
        String crossCallLog = crossCallBytes.toString(java.nio.charset.StandardCharsets.UTF_8);
        try {
            same(existingEnabled, 1, "existing identity fixture enabled count");
            same(crossCallEnabled, 2, "cross-call identity collision isolation");
            expect(existing != null
                    && foton.PluginHost.byName("existingprovider") == existing
                    && foton.PluginHost.byName("EXISTINGALIAS") == existing,
                "cross-call collision split real-name and alias lookup");
            same(csvProperty("foton.fixture.dependencies.load"), java.util.List.of(
                "ExistingProvider", "CollisionDependent", "CollisionHealthy"),
                "cross-call collisions loaded a conflicting plugin");
            same(System.getProperty("foton.fixture.dependencies.crossCallProvider"),
                "ExistingProvider", "cross-call dependency resolved the wrong provider");
            same(System.getProperty("foton.fixture.dependencies.crossCallJoined"), "true",
                "cross-call dependency classpath resolved the wrong provider");
            expect(crossCallLog.contains("existingalias")
                    && crossCallLog.contains("RealVsAlias")
                    && crossCallLog.contains("AliasVsExisting")
                    && crossCallLog.contains("AliasVsExistingAlias"),
                "cross-call collision diagnostics did not name every conflict: "
                    + crossCallLog);
        } finally {
            foton.PluginHost.disableAll();
            clearDependencyProperties();
        }
    }

    private static java.util.List<String> csvProperty(String property) {
        String value = System.getProperty(property, "");
        return value.isEmpty() ? java.util.List.of() : java.util.List.of(value.split(","));
    }

    private static void libraries(
            String directory, String malformedDirectory,
            String cachedJar, String interruptedTemporary) throws Exception {
        java.nio.file.Path cached = java.nio.file.Path.of(cachedJar);
        java.nio.file.Path interrupted = java.nio.file.Path.of(interruptedTemporary);
        byte[] before = java.nio.file.Files.readAllBytes(cached);
        byte[] interruptedBefore = java.nio.file.Files.readAllBytes(interrupted);
        System.clearProperty("foton.fixture.libraries.descriptor");
        System.clearProperty("foton.fixture.libraries.paper");
        System.clearProperty("foton.fixture.libraries.malformed");
        try {
            same(foton.PluginHost.loadAll(directory), 1,
                "pre-populated descriptor and Paper libraries should enable");
            same(System.getProperty("foton.fixture.libraries.descriptor"),
                "descriptor-cache", "descriptor library cache visibility");
            same(System.getProperty("foton.fixture.libraries.paper"),
                "paper-cache", "paper-libraries.json cache visibility");
            expect(java.util.Arrays.equals(before, java.nio.file.Files.readAllBytes(cached)),
                "a valid cache entry was replaced while a truncated sibling existed");
            expect(java.util.Arrays.equals(interruptedBefore,
                    java.nio.file.Files.readAllBytes(interrupted)),
                "an unrelated interrupted cache sibling was published or modified");
        } finally {
            foton.PluginHost.disableAll();
        }

        java.io.ByteArrayOutputStream malformedBytes = new java.io.ByteArrayOutputStream();
        java.io.PrintStream original = System.out;
        int enabled;
        try (java.io.PrintStream capture = new java.io.PrintStream(
                malformedBytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            enabled = foton.PluginHost.loadAll(malformedDirectory);
        } finally {
            System.setOut(original);
        }
        String malformedLog = malformedBytes.toString(java.nio.charset.StandardCharsets.UTF_8);
        try {
            same(enabled, 0, "malformed Maven coordinates must be rejected");
            expect(System.getProperty("foton.fixture.libraries.malformed") == null,
                "a plugin with a path-traversing coordinate was loaded");
            for (String coordinate : java.util.List.of(
                    "..:group-library:1.0", "example.fixture:..:1.0",
                    "example.fixture:version-library:..", "..:paper-library:1.0")) {
                expect(malformedLog.contains(coordinate),
                    "library failure did not name malformed coordinate " + coordinate
                        + ": " + malformedLog);
            }
        } finally {
            foton.PluginHost.disableAll();
            System.clearProperty("foton.fixture.libraries.malformed");
        }

        java.lang.reflect.Method publish = foton.PluginHost.class.getDeclaredMethod(
            "publishLibrary", java.nio.file.Path.class, java.nio.file.Path.class);
        publish.setAccessible(true);
        try {
            publish.invoke(null, interrupted, cached);
            throw new AssertionError("a truncated temporary jar was published");
        } catch (java.lang.reflect.InvocationTargetException expected) {
            expect(expected.getCause() instanceof java.io.IOException,
                "truncated publication failed for the wrong reason: " + expected.getCause());
        }
        expect(java.util.Arrays.equals(before, java.nio.file.Files.readAllBytes(cached)),
            "a truncated temporary jar replaced a valid cache entry");

        java.lang.reflect.Method createTemporary = foton.PluginHost.class.getDeclaredMethod(
            "createLibraryTemporary", java.nio.file.Path.class);
        createTemporary.setAccessible(true);
        java.nio.file.Path first = null;
        java.nio.file.Path second = null;
        try {
            first = (java.nio.file.Path) createTemporary.invoke(null, cached);
            second = (java.nio.file.Path) createTemporary.invoke(null, cached);
            expect(!first.equals(second), "library downloads reused a temporary path");
            same(first.getParent(), cached.getParent(),
                "library temporary file must be a sibling of its destination");
            same(second.getParent(), cached.getParent(),
                "second library temporary file must be a sibling of its destination");
            expect(java.nio.file.Files.isRegularFile(first)
                    && java.nio.file.Files.isRegularFile(second),
                "library temporary paths were not exclusively created");
        } finally {
            if (first != null) java.nio.file.Files.deleteIfExists(first);
            if (second != null) java.nio.file.Files.deleteIfExists(second);
        }
    }

    private static void clearDependencyProperties() {
        System.clearProperty("foton.fixture.dependencies.load");
        System.clearProperty("foton.fixture.dependencies.enable");
        System.clearProperty("foton.fixture.dependencies.alias");
        System.clearProperty("foton.fixture.dependencies.joined");
        System.clearProperty("foton.fixture.dependencies.isolated");
        System.clearProperty("foton.fixture.dependencies.afterJoined");
        System.clearProperty("foton.fixture.dependencies.omitJoined");
        System.clearProperty("foton.fixture.dependencies.crossCallProvider");
        System.clearProperty("foton.fixture.dependencies.crossCallJoined");
    }

    private static void paper(String directory, String evidencePath) throws Exception {
        for (String property : java.util.List.of(
                "foton.fixture.paper.fallback.bootstrap",
                "foton.fixture.paper.fallback.constructor",
                "foton.fixture.paper.fallback.enable",
                "foton.fixture.paper.failing.bootstrap",
                "foton.fixture.paper.invalid.main",
                "foton.fixture.paper.invalid.staticInitializer",
                "foton.fixture.paper.loader.bootstrap",
                "foton.fixture.paper.healthy")) {
            System.clearProperty(property);
        }

        java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
        java.io.PrintStream original = System.out;
        int loaded;
        int enabled;
        try (java.io.PrintStream capture = new java.io.PrintStream(
                bytes, true, java.nio.charset.StandardCharsets.UTF_8)) {
            System.setOut(capture);
            loaded = foton.PluginHost.loadAllOnLoad(directory);
            enabled = foton.PluginHost.enableAll();
        } finally {
            System.setOut(original);
        }
        String log = bytes.toString(java.nio.charset.StandardCharsets.UTF_8);
        try {
            same(loaded, 3,
                "Paper bootstrap fixtures loaded count; log=" + log);
            same(enabled, 3,
                "Paper bootstrap fixtures enabled count; log=" + log);
            org.bukkit.plugin.Plugin paper = foton.PluginHost.byName("PaperFixture");
            expect(paper != null, "Paper bootstrap fixture did not enable");
            Object steps = paper.getClass().getField("steps").get(null);
            same(steps, java.util.List.of(
                "bootstrap", "createPlugin", "constructor:sentinel", "onLoad",
                "commands", "constructorCommands", "onEnable"),
                "Paper bootstrap and constructor lifecycle order");
            expect(org.bukkit.Bukkit.getServer().getPluginCommand("paperfixture") != null,
                "bootstrap lifecycle command was not registered");
            expect(org.bukkit.Bukkit.getServer().getPluginCommand("constructorfixture") != null,
                "constructor lifecycle command was not registered");
            lifecycleTransferIsIdempotent();

            same(System.getProperty("foton.fixture.paper.fallback.bootstrap"), "true",
                "null createPlugin fallback did not bootstrap");
            same(System.getProperty("foton.fixture.paper.fallback.constructor"), "true",
                "null createPlugin did not construct descriptor main");
            same(System.getProperty("foton.fixture.paper.fallback.enable"), "true",
                "null createPlugin fallback did not enable");
            same(System.getProperty("foton.fixture.paper.healthy"), "enabled",
                "healthy neighbor did not enable after invalid bootstrappers");

            expect(foton.PluginHost.byName("InvalidBootstrap") == null,
                "class not implementing PluginBootstrap was accepted");
            expect(foton.PluginHost.byName("FailingBootstrap") == null,
                "throwing bootstrapper remained registered");
            expect(foton.PluginHost.byName("UnsupportedLoader") == null,
                "unsupported descriptor loader was accepted");
            expect(System.getProperty("foton.fixture.paper.invalid.main") == null,
                "invalid bootstrap fixture constructed its main class");
            expect(System.getProperty("foton.fixture.paper.invalid.staticInitializer") == null,
                "invalid bootstrapper ran its static initializer before type checking");
            expect(System.getProperty("foton.fixture.paper.loader.bootstrap") == null,
                "unsupported loader began bootstrap");
            expect(log.contains("example.StaticInitializerBootstrap")
                    && log.contains("PluginBootstrap"),
                "invalid bootstrapper diagnostic did not name its class: " + log);
            expect(log.contains("example.CustomLoader"),
                "unsupported loader diagnostic did not name its class: " + log);

            same(System.getProperty("foton.fixture.paper.failing.bootstrap"), "true",
                "throwing bootstrapper did not run");
            expect(org.bukkit.Bukkit.getServer().getPluginCommand("leakedbootstrap") == null,
                "failed bootstrap lifecycle command leaked");
            same(foton.LifecycleDiagnostics.hostReferences("FailingBootstrap"), 0,
                "failed bootstrap retained plugin or classloader resources");
            writeFixtureEvidence(directory, evidencePath, loaded, enabled, log);
        } finally {
            foton.PluginHost.disableAll();
        }
    }

    private static void writeFixtureEvidence(
            String directory, String output, int loaded, int enabled, String log)
            throws Exception {
        java.util.List<java.util.Map<String, String>> rejections = new java.util.ArrayList<>();
        for (String line : log.lines().toList()) {
            if (!line.startsWith("[host] ") || !line.contains(".jar failed: ")) continue;
            int split = line.indexOf(" failed: ");
            rejections.add(java.util.Map.of(
                "fixture", line.substring("[host] ".length(), split),
                "reason", line.substring(split + " failed: ".length())));
        }
        rejections.sort(java.util.Comparator.comparing(row -> row.get("fixture")));
        long discovered;
        try (var entries = java.nio.file.Files.list(java.nio.file.Path.of(directory))) {
            discovered = entries.filter(path -> path.getFileName().toString().endsWith(".jar"))
                .count();
        }
        same(rejections.size(), (int) (discovered - enabled),
            "fixture rejection diagnostics count");

        StringBuilder json = new StringBuilder();
        json.append("{\n")
            .append("  \"discovered\": ").append(discovered).append(",\n")
            .append("  \"enabled\": ").append(enabled).append(",\n")
            .append("  \"loaded\": ").append(loaded).append(",\n")
            .append("  \"rejected\": ").append(discovered - enabled).append(",\n")
            .append("  \"rejections\": [\n");
        for (int index = 0; index < rejections.size(); index++) {
            var rejection = rejections.get(index);
            json.append("    {\n")
                .append("      \"fixture\": \"")
                .append(jsonString(rejection.get("fixture"))).append("\",\n")
                .append("      \"reason\": \"")
                .append(jsonString(rejection.get("reason"))).append("\"\n")
                .append("    }");
            if (index + 1 < rejections.size()) json.append(',');
            json.append('\n');
        }
        json.append("  ]\n}\n");
        java.nio.file.Files.writeString(
            java.nio.file.Path.of(output), json, java.nio.charset.StandardCharsets.UTF_8);
    }

    private static String jsonString(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"")
            .replace("\n", "\\n").replace("\r", "\\r").replace("\t", "\\t");
    }

    private static void lifecycleTransferIsIdempotent() {
        var bootstrap =
            new io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager();
        var plugin =
            new io.papermc.paper.plugin.lifecycle.event.FotonLifecycleEventManager();
        java.util.List<String> calls = new java.util.ArrayList<>();
        bootstrap.registerEventHandler(
            io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS,
            event -> calls.add("bootstrap"));
        plugin.registerEventHandler(
            io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS,
            event -> calls.add("constructor"));

        bootstrap.transferTo(plugin);
        bootstrap.transferTo(plugin);
        var event = (io.papermc.paper.plugin.lifecycle.event.registrar.ReloadableRegistrarEvent)
            () -> new foton.FotonCommands();
        plugin.dispatch(
            io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS, event);
        plugin.dispatch(
            io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents.COMMANDS, event);
        same(calls, java.util.List.of(
            "bootstrap", "constructor", "bootstrap", "constructor"),
            "repeated lifecycle transfer or dispatch duplicated a bootstrap handler");
    }

    private static void awaitSchedulerTasks(String owner, int expected) {
        long deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(5);
        while (foton.LifecycleDiagnostics.schedulerTasks(owner) != expected) {
            if (System.nanoTime() >= deadline) {
                throw new AssertionError("scheduler did not release the old " + owner
                    + " generation");
            }
            Thread.onSpinWait();
        }
    }

}
