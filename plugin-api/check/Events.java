/** The event path, and the scheduler's promise about when tasks run. */
final class Events {
    private Events() {}

    static void check(String pluginDirectory) throws Exception {
        // An operator's own config.yml, written before `added-later` existed.
        // The jar's copy has to answer for the key this file does not have,
        // which is what makes a plugin update work rather than silently read
        // every new setting as zero.
        java.io.File data = new java.io.File(pluginDirectory, "EventFixture");
        java.nio.file.Files.createDirectories(data.toPath());
        java.nio.file.Files.writeString(new java.io.File(data, "config.yml").toPath(),
            "greeting: from the operator\n");

        Checks.expect(foton.PluginHost.loadAll(pluginDirectory) == 1,
            "the fixture plugin should have enabled");

        Checks.same(example.EventFixture.greeting, "from the operator",
            "the operator's own config should win");
        Checks.same(example.EventFixture.addedLater, 42,
            "a key only the jar has should read from the jar");
        Checks.same(example.EventFixture.nested, "deep",
            "a nested default should be reachable by path");

        String id = "00000000-0000-0000-0000-000000000001";

        Checks.same(foton.EventBridge.fireJoin(id, "original"), "rewritten by the fixture",
            "a handler's rewrite did not travel back");

        Checks.expect(foton.EventBridge.fireChat(id, "hush now") == null,
            "a cancelled chat should come back as nothing");
        Checks.same(foton.EventBridge.fireChat(id, "hello"), "hello",
            "an uncancelled chat should come back unchanged");

        // The LOWEST handler cancels; the HIGH one would undo it but did not
        // ask to see cancelled events, so it must never run.
        Checks.expect(!foton.EventBridge.fireBlockBreak(id, 1, 2, 3, "minecraft:overworld"),
            "a cancelled break was reported as allowed");

        scheduler();
        foton.PluginHost.disableAll();
    }

    /** The scheduler's promise is about *when*, so this checks when. */
    private static void scheduler() {
        // Three tasks were queued in onEnable. None may have run: a body that
        // runs at submission time is a plugin touching the world from whatever
        // thread called in, which is the bug this design exists to prevent.
        Checks.expect(example.EventFixture.immediate == 0
            && example.EventFixture.delayed == 0
            && example.EventFixture.repeating == 0, "a task ran before any tick");

        // Tick 1: the immediate one, and the repeating one's first run.
        Checks.same(foton.FotonScheduler.tick(), 2, "the first tick owed two tasks");
        Checks.same(example.EventFixture.immediate, 1, "runTask missed the first tick");
        Checks.same(example.EventFixture.delayed, 0, "a delayed task ran early");

        // Tick 2 owes the delayed task; tick 3 owes the repeat's second run.
        foton.FotonScheduler.tick();
        foton.FotonScheduler.tick();
        Checks.same(example.EventFixture.delayed, 1, "runTaskLater missed its tick");
        Checks.same(example.EventFixture.repeating, 2, "runTaskTimer did not repeat");

        // A task that runs once must not run twice.
        foton.FotonScheduler.tick();
        foton.FotonScheduler.tick();
        Checks.expect(example.EventFixture.immediate == 1 && example.EventFixture.delayed == 1,
            "a one-shot task ran again");
        // Five ticks, a period of two: runs on 1, 3 and 5.
        Checks.same(example.EventFixture.repeating, 3, "the repeat lost its period");
    }
}
