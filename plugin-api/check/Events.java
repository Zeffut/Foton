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

        int[] ignoredCancellation = {0};
        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() {};
        org.bukkit.plugin.Plugin owner = foton.PluginHost.all()[0];
        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.player.AsyncPlayerChatEvent.class,
            listener,
            org.bukkit.event.EventPriority.NORMAL,
            (registered, event) -> ignoredCancellation[0]++,
            owner,
            true);
        org.bukkit.event.player.AsyncPlayerChatEvent cancelled =
            new org.bukkit.event.player.AsyncPlayerChatEvent(null, "cancelled");
        cancelled.setCancelled(true);
        org.bukkit.Bukkit.getPluginManager().callEvent(cancelled);
        Checks.same(ignoredCancellation[0], 0,
            "registerEvent(ignoreCancelled=true) should ignore a cancelled event");

        org.bukkit.Bukkit.getPluginManager().registerEvent(
            org.bukkit.event.player.AsyncPlayerChatEvent.class,
            listener,
            org.bukkit.event.EventPriority.NORMAL,
            (registered, event) -> ignoredCancellation[0]++,
            owner,
            false);
        org.bukkit.Bukkit.getPluginManager().callEvent(cancelled);
        Checks.same(ignoredCancellation[0], 1,
            "registerEvent(ignoreCancelled=false) should receive a cancelled event");

        pluginMessages(owner);
        scheduler();
    }

    private static void pluginMessages(org.bukkit.plugin.Plugin owner) {
        org.bukkit.plugin.messaging.Messenger messenger = org.bukkit.Bukkit.getMessenger();
        int[] heard = {0};
        org.bukkit.plugin.messaging.PluginMessageListener listener =
            (channel, player, message) -> {
                Checks.same(channel, "fixture:messages", "the plugin channel changed in transit");
                Checks.same(message[0], (byte) 7, "the plugin payload changed in transit");
                heard[0]++;
            };
        org.bukkit.plugin.messaging.PluginMessageListenerRegistration registration =
            messenger.registerIncomingPluginChannel(owner, "fixture:messages", listener);
        Checks.expect(registration.isValid(), "a fresh plugin channel registration is invalid");
        Checks.expect(messenger.getIncomingChannels(owner).contains("fixture:messages"),
            "the plugin's incoming channel was not recorded");

        messenger.dispatchIncomingMessage(
            new foton.FotonPlayer(java.util.UUID.fromString(
                "00000000-0000-0000-0000-000000000001")),
            "fixture:messages",
            new byte[] {7, 8});
        Checks.same(heard[0], 1, "an incoming plugin message missed its listener");

        boolean duplicateRejected = false;
        try {
            messenger.registerIncomingPluginChannel(owner, "fixture:messages", listener);
        } catch (IllegalArgumentException expected) {
            duplicateRejected = true;
        }
        Checks.expect(duplicateRejected, "a duplicate plugin channel listener was accepted");

        messenger.registerOutgoingPluginChannel(owner, "fixture:messages");
        Checks.expect(messenger.isOutgoingChannelRegistered(owner, "fixture:messages"),
            "the outgoing plugin channel was not recorded");
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

        foliaTaskState(owner());
    }

    /** Folia exposes the lifecycle and the outcome of cancellation to plugins. */
    private static void foliaTaskState(org.bukkit.plugin.Plugin owner) {
        io.papermc.paper.threadedregions.scheduler.GlobalRegionScheduler scheduler =
            org.bukkit.Bukkit.getGlobalRegionScheduler();

        io.papermc.paper.threadedregions.scheduler.ScheduledTask cancelled =
            scheduler.runDelayed(owner, ignored -> {
                throw new AssertionError("a cancelled Folia task ran");
            }, 2);
        Checks.same(cancelled.getExecutionState(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState.IDLE,
            "a queued Folia task should be idle");
        Checks.same(cancelled.cancel(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.CancelledState
                .CANCELLED_BY_CALLER,
            "the first cancellation should cancel an idle task");
        Checks.expect(cancelled.isCancelled(), "a cancelled Folia task should say so");
        Checks.same(cancelled.cancel(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.CancelledState
                .CANCELLED_ALREADY,
            "a second cancellation should report the existing cancellation");

        io.papermc.paper.threadedregions.scheduler.ScheduledTask[] observed = {null};
        io.papermc.paper.threadedregions.scheduler.ScheduledTask once = scheduler.run(owner, task -> {
            observed[0] = task;
            Checks.same(task.getExecutionState(),
                io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState.RUNNING,
                "a Folia task body should observe itself running");
        });
        Checks.expect(!once.isRepeatingTask(), "a one-shot Folia task reported as repeating");
        foton.FotonScheduler.tick();
        Checks.expect(observed[0] == once, "the Folia body received a different task handle");
        Checks.same(once.getExecutionState(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState.FINISHED,
            "a one-shot Folia task should finish after its body");
        Checks.same(once.cancel(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.CancelledState.ALREADY_EXECUTED,
            "a finished Folia task cannot be cancelled retroactively");

        int[] runs = {0};
        io.papermc.paper.threadedregions.scheduler.ScheduledTask repeating =
            scheduler.runAtFixedRate(owner, task -> {
                runs[0]++;
                Checks.same(task.cancel(),
                    io.papermc.paper.threadedregions.scheduler.ScheduledTask.CancelledState
                        .NEXT_RUNS_CANCELLED,
                    "cancelling a running repeat should cancel its next runs");
                Checks.same(task.getExecutionState(),
                    io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState
                        .CANCELLED_RUNNING,
                    "a running repeat should expose its pending cancellation");
            }, 0, 1);
        Checks.expect(repeating.isRepeatingTask(), "a repeating Folia task reported as one-shot");
        foton.FotonScheduler.tick();
        Checks.same(repeating.getExecutionState(),
            io.papermc.paper.threadedregions.scheduler.ScheduledTask.ExecutionState.CANCELLED,
            "a cancelled repeat should settle after its body");
        foton.FotonScheduler.tick();
        Checks.same(runs[0], 1, "a cancelled Folia repeat ran again");
    }

    private static org.bukkit.plugin.Plugin owner() {
        return foton.PluginHost.all()[0];
    }
}
