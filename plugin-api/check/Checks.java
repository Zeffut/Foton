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
