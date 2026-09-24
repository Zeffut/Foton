/** Hostile lifecycle cases that otherwise only appear during reloads or failures. */
final class PluginLifecycle {
    private PluginLifecycle() {}

    static void check() throws Exception {
        org.bukkit.plugin.Plugin fixture = foton.PluginHost.byName("EventFixture");
        eventBridgeRace(fixture);
        eventBridgeRemovesEmptyType(fixture);
        eventBridgeClearRace(fixture);

        java.nio.file.Path root = java.nio.file.Files.createTempDirectory(
            "foton-plugin-lifecycle-");
        try {
            teardown(root.resolve("teardown"));
            invocationBarrier(root.resolve("invocation-barrier"));
            selfDisableDoesNotWaitOnItself(root.resolve("self-disable"));
            schedulerBarrier(root.resolve("scheduler-barrier"));
            failedDependency(root.resolve("dependency"));
            allPluginsPublishedBeforeOnLoad(root.resolve("two-phase-load"));
            failedOnLoadCleansDependents(root.resolve("failed-on-load"));
            loadBefore(root.resolve("load-before"));
            softCycleDoesNotPoisonHardDependent(root.resolve("soft-cycle"));
            concurrentLoadAndDisable(root.resolve("concurrent-load"));
            crossPluginDisableDoesNotDeadlock(root.resolve("cross-disable"));
            lifecycleCallbackCanJoinHelper(root.resolve("lifecycle-helper"));
            disableEventSeesPublishedResources(root.resolve("disable-visibility"));
            oldTeardownCannotCloseReplacement(root.resolve("reload-identity"));
            stoppingIsVisibleDuringDisable(root.resolve("stopping"));
        } finally {
            discard("LifecycleTeardown");
            discard("AlphaTarget");
            discard("ZuluBefore");
            discard("SoftCycleAlpha");
            discard("SoftCycleBeta");
            discard("HardAfterSoftCycle");
            discard("InvocationBarrier");
            discard("InvocationCompanion");
            discard("SchedulerBarrier");
            discard("SelfDisable");
            discard("PhaseAlpha");
            discard("PhaseZulu");
            discard("ConcurrentLifecycle");
            discard("DisableObserver");
            discard("DisableTarget");
            discard("ReloadIdentity");
            discard("CrossDisableAlpha");
            discard("CrossDisableBeta");
            discard("LifecycleHelper");
            discard("LifecycleHelperCompanion");
            discard("StoppingObserver");
            try (java.util.stream.Stream<java.nio.file.Path> paths =
                     java.nio.file.Files.walk(root)) {
                for (java.nio.file.Path path : paths.sorted(java.util.Comparator.reverseOrder())
                        .toList()) {
                    java.nio.file.Files.deleteIfExists(path);
                }
            }
        }
    }

    static void concurrentDisableAllKeepsLoaderAlive() throws Exception {
        java.nio.file.Path root = java.nio.file.Files.createTempDirectory(
            "foton-disable-all-lifecycle-");
        DisableAllBarrierPlugin.reset();
        Thread normalDisable = null;
        try {
            pluginJar(root, "DisableAllBarrier", DisableAllBarrierPlugin.class, "");
            // Not a count: like Bukkit's enablePlugins, loadAll also re-enables
            // plugins an earlier check disabled without discarding.
            foton.PluginHost.loadAll(root.toString());
            org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("DisableAllBarrier");
            Checks.expect(plugin != null && plugin.isEnabled(),
                "the disable-all barrier fixture should enable");
            org.bukkit.plugin.java.PluginClassLoader loader = pluginLoader("DisableAllBarrier");

            normalDisable = new Thread(() -> foton.PluginHost.disable(plugin),
                "normal-disable-before-disable-all");
            normalDisable.start();
            Checks.expect(DisableAllBarrierPlugin.entered.await(5,
                    java.util.concurrent.TimeUnit.SECONDS),
                "onDisable did not reach the classloader barrier");

            Thread shutdown = new Thread(foton.PluginHost::disableAll,
                "disable-all-during-on-disable");
            shutdown.start();
            join(shutdown, "disableAll deadlocked behind a running onDisable callback");

            Checks.expect(foton.PluginHost.byName("DisableAllBarrier") == plugin
                    && pluginLoader("DisableAllBarrier") == loader,
                "disableAll unpublished the plugin before onDisable returned");
            try (java.io.InputStream resource = loader.getResourceAsStream("plugin.yml")) {
                Checks.expect(resource != null,
                    "disableAll closed the classloader while onDisable was running");
            }
        } finally {
            DisableAllBarrierPlugin.release.countDown();
            if (normalDisable != null) {
                join(normalDisable, "normal disable did not finalize deferred discard");
            }
            discard("DisableAllBarrier");
            try (java.util.stream.Stream<java.nio.file.Path> paths =
                     java.nio.file.Files.walk(root)) {
                for (java.nio.file.Path path : paths.sorted(java.util.Comparator.reverseOrder())
                        .toList()) {
                    java.nio.file.Files.deleteIfExists(path);
                }
            }
        }
        Checks.expect(DisableAllBarrierPlugin.resourceAccessible,
            "the plugin could not read its own resource before onDisable returned");
        Checks.expect(foton.PluginHost.byName("DisableAllBarrier") == null,
            "deferred disableAll discard did not unpublish the plugin");
    }

    private static void eventBridgeRemovesEmptyType(org.bukkit.plugin.Plugin owner) {
        int baseline = foton.EventBridge.handlerTypeCount();
        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() {};
        foton.EventBridge.register(listener, LifecycleEvent.class,
            org.bukkit.event.EventPriority.NORMAL, (registered, event) -> {}, owner);
        Checks.same(foton.EventBridge.handlerTypeCount(), baseline + 1,
            "registering a new event type did not create one key");
        foton.EventBridge.unregister(listener);
        Checks.same(foton.EventBridge.handlerTypeCount(), baseline,
            "unregister retained an empty event-type key");
    }

    private static void invocationBarrier(java.nio.file.Path plugins) throws Exception {
        InvocationBarrierPlugin.reset();
        pluginJar(plugins, "InvocationBarrier", InvocationBarrierPlugin.class, "");
        pluginJar(plugins, "InvocationCompanion", InvocationCompanionPlugin.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 2,
            "the invocation-barrier fixtures should enable");
        org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("InvocationBarrier");

        Thread dispatch = new Thread(() -> foton.EventBridge.dispatch(new LifecycleEvent()),
            "in-flight-plugin-callback");
        dispatch.start();
        InvocationBarrierPlugin.entered.await();
        Thread disable = new Thread(() -> foton.PluginHost.disable(plugin),
            "disable-with-in-flight-callback");
        disable.start();
        awaitDisabled(plugin);

        foton.EventBridge.dispatch(new LifecycleEvent());
        Checks.same(InvocationBarrierPlugin.calls.get(), 1,
            "disable admitted a new callback after closing the invocation barrier");
        Checks.expect(disable.isAlive(),
            "disable closed the plugin while its callback was still in flight");
        InvocationBarrierPlugin.release.countDown();
        join(dispatch, "the callback deadlocked while re-entering the lifecycle host");
        join(disable, "disable did not finish after the reentrant callback drained");
        Checks.expect(InvocationBarrierPlugin.reentrantLifecycleReturned,
            "the active callback could not re-enter the lifecycle host");
        Checks.expect(!foton.PluginHost.byName("InvocationCompanion").isEnabled(),
            "the lifecycle call made by the active callback did not complete");
        Checks.expect(!plugin.isEnabled(), "disabled state was not visible across threads");
        discard("InvocationBarrier");
        discard("InvocationCompanion");
    }

    private static void schedulerBarrier(java.nio.file.Path plugins) throws Exception {
        SchedulerBarrierPlugin.reset();
        pluginJar(plugins, "SchedulerBarrier", SchedulerBarrierPlugin.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
            "the scheduler-barrier fixture should enable");
        org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("SchedulerBarrier");
        SchedulerBarrierPlugin.entered.await();

        Thread disable = new Thread(() -> foton.PluginHost.disable(plugin),
            "disable-with-in-flight-task");
        disable.start();
        awaitDisabled(plugin);
        Checks.same(foton.FotonScheduler.tick(), 0,
            "a queued task began after plugin disable won the lifecycle lock");
        Checks.same(SchedulerBarrierPlugin.queuedRuns.get(), 0,
            "cancel and task start were not atomic with plugin disable");
        Checks.expect(disable.isAlive(),
            "disable did not wait for the running async task");

        SchedulerBarrierPlugin.release.countDown();
        disable.join();
        Checks.same(SchedulerBarrierPlugin.asyncRuns.get(), 1,
            "the running async task did not finish exactly once");
        discard("SchedulerBarrier");
    }

    private static void selfDisableDoesNotWaitOnItself(java.nio.file.Path plugins)
            throws Exception {
        SelfDisablePlugin.returned = false;
        pluginJar(plugins, "SelfDisable", SelfDisablePlugin.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
            "the self-disable fixture should enable");
        org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("SelfDisable");
        Thread dispatch = new Thread(() -> foton.EventBridge.dispatch(new LifecycleEvent()),
            "self-disabling-callback");
        dispatch.start();
        dispatch.join(java.util.concurrent.TimeUnit.SECONDS.toMillis(5));
        Checks.expect(!dispatch.isAlive(), "a callback deadlocked while disabling its own plugin");
        Checks.expect(SelfDisablePlugin.returned,
            "self-disable did not return to the callback before deferred classloader close");
        Checks.expect(foton.PluginHost.byName("SelfDisable") == plugin,
            "normal self-disable removed the Paper-visible plugin identity");
        discard("SelfDisable");
    }

    private static void awaitDisabled(org.bukkit.plugin.Plugin plugin) throws Exception {
        long deadline = System.nanoTime() + java.util.concurrent.TimeUnit.SECONDS.toNanos(5);
        while (plugin.isEnabled() && System.nanoTime() < deadline) Thread.onSpinWait();
        Checks.expect(!plugin.isEnabled(), "plugin disable did not publish its lifecycle state");
    }

    private static void teardown(java.nio.file.Path plugins) throws Exception {
        TeardownPlugin.reset();
        pluginJar(plugins, "LifecycleTeardown", TeardownPlugin.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
            "the teardown fixture should enable");

        org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("LifecycleTeardown");
        Checks.expect(plugin != null, "the teardown fixture disappeared before disable");
        foton.PluginHost.disable(plugin);

        Checks.same(TeardownPlugin.disableCalls, 1, "onDisable should run exactly once");
        Checks.expect(!TeardownPlugin.enabledDuringDisable,
            "a plugin should already report disabled from onDisable");
        Checks.expect(!TeardownPlugin.syncCancelledDuringDisable,
            "the host tore down tasks before onDisable");
        Checks.same(TeardownPlugin.handlersDuringDisable, 1,
            "the host tore down listeners before onDisable");
        Checks.expect(TeardownPlugin.rejectedTaskDuringDisable,
            "onDisable was allowed to create a task that could escape teardown");
        Checks.expect(TeardownPlugin.sync.isCancelled() && TeardownPlugin.async.isCancelled(),
            "a plugin task survived disable");
        Checks.same(foton.EventBridge.handlerCount(
                org.bukkit.event.player.AsyncPlayerChatEvent.class.getName()),
            TeardownPlugin.handlerBaseline,
            "a disabled plugin left an event handler registered");
        foton.FotonScheduler.tick();
        Checks.same(TeardownPlugin.runs, 0, "a disabled plugin task still executed");
        Checks.expect(foton.PluginHost.byName("LifecycleTeardown") == plugin,
            "normal disable removed the Paper-visible plugin identity");
        discard("LifecycleTeardown");
    }

    private static void failedDependency(java.nio.file.Path plugins) throws Exception {
        FailingProvider.reset();
        DependentPlugin.reset();
        pluginJar(plugins, "FailingProvider", FailingProvider.class, "");
        pluginJar(plugins, "DependentAfterFailure", DependentPlugin.class,
            "depend: [FailingProvider]\n");

        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 0,
            "a failed provider or its required dependent was enabled");
        Checks.same(FailingProvider.enableCalls, 1,
            "the failing provider did not reach its hostile onEnable");
        Checks.same(FailingProvider.disableCalls, 1,
            "a provider whose onEnable failed was not disabled cleanly");
        Checks.same(DependentPlugin.enableCalls, 0,
            "a dependent enabled after its required provider failed");
        Checks.same(DependentPlugin.disableCalls, 0,
            "onDisable ran for a dependent that was never enabled");
        Checks.expect(foton.PluginHost.byName("FailingProvider") != null
                && foton.PluginHost.byName("DependentAfterFailure") != null,
            "enable failure discarded plugins instead of leaving them disabled");
        discard("DependentAfterFailure");
        discard("FailingProvider");
    }

    private static void loadBefore(java.nio.file.Path plugins) throws Exception {
        LOAD_ORDER.clear();
        // Alphabetical discovery puts Alpha first unless Zulu's loadbefore is
        // translated into the inverse ordering edge.
        pluginJar(plugins, "AlphaTarget", TargetPlugin.class, "");
        pluginJar(plugins, "ZuluBefore", BeforePlugin.class,
            "loadbefore: [AlphaTarget]\n");

        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 2,
            "the loadbefore fixtures should both enable");
        Checks.same(java.util.List.copyOf(LOAD_ORDER), java.util.List.of(
                "before-load", "target-load", "before-enable", "target-enable"),
            "loadbefore did not order both lifecycle phases");

        foton.PluginHost.disable(foton.PluginHost.byName("AlphaTarget"));
        foton.PluginHost.disable(foton.PluginHost.byName("ZuluBefore"));
        discard("AlphaTarget");
        discard("ZuluBefore");
    }

    private static void softCycleDoesNotPoisonHardDependent(java.nio.file.Path plugins)
            throws Exception {
        SOFT_CYCLE_LOAD_ORDER.clear();
        pluginJar(plugins, "SoftCycleAlpha", SoftCycleAlpha.class,
            "softdepend: [SoftCycleBeta]\n");
        pluginJar(plugins, "SoftCycleBeta", SoftCycleBeta.class,
            "softdepend: [SoftCycleAlpha]\n");
        pluginJar(plugins, "HardAfterSoftCycle", HardAfterSoftCycle.class,
            "depend: [SoftCycleAlpha]\n");

        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 3,
            "a soft-dependency cycle poisoned a valid hard dependent");
        Checks.expect(SOFT_CYCLE_LOAD_ORDER.containsAll(java.util.List.of(
                "alpha", "beta", "hard")) && SOFT_CYCLE_LOAD_ORDER.size() == 3,
            "the soft-cycle fixtures did not each load exactly once");
        Checks.expect(SOFT_CYCLE_LOAD_ORDER.indexOf("alpha")
                < SOFT_CYCLE_LOAD_ORDER.indexOf("hard"),
            "a hard dependent loaded before its required plugin");

        discard("HardAfterSoftCycle");
        discard("SoftCycleBeta");
        discard("SoftCycleAlpha");
    }

    private static void allPluginsPublishedBeforeOnLoad(java.nio.file.Path plugins)
            throws Exception {
        PhaseAlpha.sawZulu = false;
        PhaseAlpha.enabledDuringLoad = true;
        pluginJar(plugins, "PhaseAlpha", PhaseAlpha.class, "");
        pluginJar(plugins, "PhaseZulu", PhaseZulu.class, "");

        Checks.same(foton.PluginHost.loadAllOnLoad(plugins.toString()), 2,
            "both two-phase fixtures should finish onLoad");
        Checks.expect(PhaseAlpha.sawZulu,
            "the first onLoad could not see a later constructed plugin");
        Checks.expect(!PhaseAlpha.enabledDuringLoad,
            "a plugin was marked enabled before the enable phase");
        Checks.same(foton.PluginHost.enableAll(), 2,
            "both two-phase fixtures should enable after onLoad");

        foton.PluginHost.disable(foton.PluginHost.byName("PhaseZulu"));
        foton.PluginHost.disable(foton.PluginHost.byName("PhaseAlpha"));
        discard("PhaseZulu");
        discard("PhaseAlpha");
    }

    private static void failedOnLoadCleansDependents(java.nio.file.Path plugins)
            throws Exception {
        FailingOnLoad.disableCalls = 0;
        DependentOnLoad.loadCalls = 0;
        pluginJar(plugins, "FailingOnLoad", FailingOnLoad.class,
            "commands:\n  doomed:\n    description: must be released\n");
        pluginJar(plugins, "DependentOnLoad", DependentOnLoad.class,
            "depend: [FailingOnLoad]\n");

        Checks.same(foton.PluginHost.loadAllOnLoad(plugins.toString()), 0,
            "an onLoad failure or its dependent was retained");
        Checks.same(DependentOnLoad.loadCalls, 0,
            "a dependent ran onLoad after its required provider failed onLoad");
        Checks.same(FailingOnLoad.disableCalls, 0,
            "onDisable ran for a plugin that never reached the enable phase");
        Checks.expect(foton.PluginHost.byName("FailingOnLoad") == null
                && foton.PluginHost.byName("DependentOnLoad") == null,
            "an onLoad failure left plugins published");
        Checks.expect(foton.CommandMap.get("doomed") == null,
            "an onLoad failure retained a command registered during construction");
    }

    private static void concurrentLoadAndDisable(java.nio.file.Path plugins)
            throws Exception {
        ConcurrentLifecyclePlugin.reset();
        pluginJar(plugins, "ConcurrentLifecycle", ConcurrentLifecyclePlugin.class, "");

        java.util.concurrent.CountDownLatch loadStart =
            new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicInteger loaded =
            new java.util.concurrent.atomic.AtomicInteger();
        java.util.concurrent.atomic.AtomicReference<Throwable> loadFailure =
            new java.util.concurrent.atomic.AtomicReference<>();
        Thread firstLoad = lifecycleThread("concurrent-plugin-load-1", loadStart, loadFailure,
            () -> loaded.addAndGet(foton.PluginHost.loadAll(plugins.toString())));
        Thread secondLoad = lifecycleThread("concurrent-plugin-load-2", loadStart, loadFailure,
            () -> loaded.addAndGet(foton.PluginHost.loadAll(plugins.toString())));
        firstLoad.start();
        secondLoad.start();
        loadStart.countDown();
        join(firstLoad, "the first concurrent plugin load deadlocked");
        join(secondLoad, "the second concurrent plugin load deadlocked");

        Checks.expect(loadFailure.get() == null,
            "concurrent plugin load failed: " + loadFailure.get());
        Checks.same(loaded.get(), 1,
            "concurrent discovery published or enabled the same plugin twice");
        Checks.same(ConcurrentLifecyclePlugin.loadCalls.get(), 1,
            "concurrent discovery invoked onLoad more than once");
        Checks.same(ConcurrentLifecyclePlugin.enableCalls.get(), 1,
            "concurrent discovery invoked onEnable more than once");
        long published = java.util.Arrays.stream(foton.PluginHost.all())
            .filter(plugin -> plugin.getName().equalsIgnoreCase("ConcurrentLifecycle"))
            .count();
        Checks.same(published, 1L,
            "concurrent discovery published duplicate plugin identities");

        org.bukkit.plugin.Plugin plugin = foton.PluginHost.byName("ConcurrentLifecycle");
        java.util.concurrent.CountDownLatch disableStart =
            new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<Throwable> disableFailure =
            new java.util.concurrent.atomic.AtomicReference<>();
        Thread firstDisable = lifecycleThread("concurrent-plugin-disable-1", disableStart,
            disableFailure, () -> foton.PluginHost.disable(plugin));
        Thread secondDisable = lifecycleThread("concurrent-plugin-disable-2", disableStart,
            disableFailure, () -> foton.PluginHost.disable(plugin));
        firstDisable.start();
        secondDisable.start();
        disableStart.countDown();
        join(firstDisable, "the first concurrent plugin disable deadlocked");
        join(secondDisable, "the second concurrent plugin disable deadlocked");

        Checks.expect(disableFailure.get() == null,
            "concurrent plugin disable failed: " + disableFailure.get());
        Checks.same(ConcurrentLifecyclePlugin.disableCalls.get(), 1,
            "concurrent disable invoked onDisable more than once");
        Checks.expect(ConcurrentLifecyclePlugin.reentrantEnableReturned,
            "onDisable deadlocked while re-entering the lifecycle host");
        Checks.expect(foton.PluginHost.byName("ConcurrentLifecycle") == plugin,
            "normal concurrent disable unpublished the plugin");
        Checks.expect(!plugin.isEnabled(),
            "normal concurrent disable left the plugin enabled");
        discard("ConcurrentLifecycle");
    }

    private static void crossPluginDisableDoesNotDeadlock(java.nio.file.Path plugins)
            throws Exception {
        CrossDisable.reset();
        pluginJar(plugins, "CrossDisableAlpha", CrossDisableAlpha.class, "");
        pluginJar(plugins, "CrossDisableBeta", CrossDisableBeta.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 2,
            "the cross-disable fixtures should enable");

        Thread alpha = new Thread(
            () -> foton.EventBridge.dispatch(new CrossDisableAlphaEvent()),
            "cross-disable-alpha-callback");
        Thread beta = new Thread(
            () -> foton.EventBridge.dispatch(new CrossDisableBetaEvent()),
            "cross-disable-beta-callback");
        alpha.start();
        beta.start();
        join(alpha, "plugin A deadlocked while disabling active plugin B");
        join(beta, "plugin B deadlocked while disabling active plugin A");

        Checks.same(CrossDisable.returned.get(), 2,
            "cross-plugin disable did not return to both callbacks");
        Checks.expect(!foton.PluginHost.byName("CrossDisableAlpha").isEnabled()
                && !foton.PluginHost.byName("CrossDisableBeta").isEnabled(),
            "cross-plugin deferred disable did not finalize both plugins");
        discard("CrossDisableBeta");
        discard("CrossDisableAlpha");
    }

    private static void lifecycleCallbackCanJoinHelper(java.nio.file.Path plugins)
            throws Exception {
        LifecycleHelperPlugin.reset();
        pluginJar(plugins, "LifecycleHelper", LifecycleHelperPlugin.class, "");
        pluginJar(plugins, "LifecycleHelperCompanion", LifecycleHelperCompanion.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 2,
            "the lifecycle-helper fixtures should enable");

        foton.PluginHost.disable(foton.PluginHost.byName("LifecycleHelper"));
        Checks.expect(LifecycleHelperPlugin.helperReturned,
            "onDisable deadlocked while joining a lifecycle helper thread");
        Checks.expect(!foton.PluginHost.byName("LifecycleHelperCompanion").isEnabled(),
            "the lifecycle helper did not disable its target");
        discard("LifecycleHelperCompanion");
        discard("LifecycleHelper");
    }

    private static void stoppingIsVisibleDuringDisable(java.nio.file.Path plugins)
            throws Exception {
        StoppingObserver.sawStopping = false;
        pluginJar(plugins, "StoppingObserver", StoppingObserver.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
            "the stopping observer should enable");

        Checks.expect(!org.bukkit.Bukkit.isStopping(),
            "ordinary plugin disable/re-enable marked the whole server as stopping");
        foton.PluginHost.markStopping();
        foton.PluginHost.disable(foton.PluginHost.byName("StoppingObserver"));
        Checks.expect(StoppingObserver.sawStopping,
            "Bukkit.isStopping was false inside shutdown onDisable");
        discard("StoppingObserver");
    }

    private static Thread lifecycleThread(String name,
            java.util.concurrent.CountDownLatch start,
            java.util.concurrent.atomic.AtomicReference<Throwable> failure,
            Runnable action) {
        return new Thread(() -> {
            try {
                start.await();
                action.run();
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        }, name);
    }

    private static void join(Thread thread, String failure) throws Exception {
        thread.join(java.util.concurrent.TimeUnit.SECONDS.toMillis(5));
        Checks.expect(!thread.isAlive(), failure);
    }

    private static void disableEventSeesPublishedResources(java.nio.file.Path plugins)
            throws Exception {
        DisableObserverPlugin.reset();
        pluginJar(plugins, "DisableObserver", DisableObserverPlugin.class, "");
        pluginJar(plugins, "DisableTarget", DisableTargetPlugin.class,
            "commands:\n  visible-during-disable:\n"
                + "    description: lifecycle visibility\n");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 2,
            "the disable-visibility fixtures should enable");

        org.bukkit.plugin.Plugin target = foton.PluginHost.byName("DisableTarget");
        org.bukkit.plugin.java.PluginClassLoader loader = pluginLoader("DisableTarget");
        int lifecycleHandlers = foton.EventBridge.handlerCount(LifecycleEvent.class.getName());
        foton.PluginHost.disable(target);

        Checks.expect(DisableObserverPlugin.sawEvent,
            "other plugins did not receive PluginDisableEvent");
        Checks.expect(DisableObserverPlugin.sawPublished,
            "PluginDisableEvent could not discover the plugin being disabled");
        Checks.expect(DisableObserverPlugin.sawCommand,
            "plugin commands disappeared before PluginDisableEvent");
        Checks.expect(DisableObserverPlugin.sawHandler,
            "plugin listeners disappeared before PluginDisableEvent");
        Checks.expect(DisableObserverPlugin.sawLiveLoader,
            "plugin resources disappeared before PluginDisableEvent");
        Checks.expect(foton.PluginHost.byName("DisableTarget") == target,
            "normal disable replaced or unpublished the plugin identity");
        Checks.expect(pluginLoader("DisableTarget") == loader,
            "normal disable replaced or closed the plugin classloader");
        Checks.expect(foton.CommandMap.get("visible-during-disable") == null,
            "plugin commands survived post-event cleanup");
        Checks.same(foton.EventBridge.handlerCount(LifecycleEvent.class.getName()),
            lifecycleHandlers - 1, "plugin listeners survived post-event cleanup");

        Checks.same(foton.PluginHost.enableAll(), 1,
            "a normally disabled plugin could not be enabled again");
        Checks.expect(foton.PluginHost.byName("DisableTarget") == target
                && pluginLoader("DisableTarget") == loader,
            "re-enable did not preserve plugin and classloader identity");
        Checks.same(foton.EventBridge.handlerCount(LifecycleEvent.class.getName()),
            lifecycleHandlers, "re-enable did not restore plugin listeners");
        foton.PluginHost.disable(target);
        discard("DisableTarget");
        discard("DisableObserver");
    }

    private static void oldTeardownCannotCloseReplacement(java.nio.file.Path plugins)
            throws Exception {
        ReloadIdentityPlugin.reset();
        pluginJar(plugins, "ReloadIdentity", ReloadIdentityPlugin.class, "");
        Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
            "the reload-identity fixture should enable");
        org.bukkit.plugin.Plugin oldPlugin = foton.PluginHost.byName("ReloadIdentity");
        org.bukkit.plugin.java.PluginClassLoader oldLoader = pluginLoader("ReloadIdentity");

        Thread callback = new Thread(
            () -> foton.EventBridge.dispatch(new LifecycleEvent()),
            "discard-during-plugin-callback");
        callback.start();
        org.bukkit.plugin.Plugin replacement;
        org.bukkit.plugin.java.PluginClassLoader replacementLoader;
        try {
            Checks.expect(ReloadIdentityPlugin.discarded.await(5,
                    java.util.concurrent.TimeUnit.SECONDS),
                "self-discard did not return before its callback drained");
            Checks.expect(ReloadIdentityPlugin.failure.get() == null,
                "self-discard failed: " + ReloadIdentityPlugin.failure.get());
            Checks.expect(foton.PluginHost.byName("ReloadIdentity") == null,
                "discard left the old plugin published");

            Checks.same(foton.PluginHost.loadAll(plugins.toString()), 1,
                "a replacement plugin could not load while old teardown was deferred");
            replacement = foton.PluginHost.byName("ReloadIdentity");
            replacementLoader = pluginLoader("ReloadIdentity");
            Checks.expect(replacement != null && replacement != oldPlugin,
                "reload reused the discarded plugin identity");
            Checks.expect(replacementLoader != null && replacementLoader != oldLoader,
                "reload reused the discarded plugin classloader");
        } finally {
            ReloadIdentityPlugin.release.countDown();
        }
        join(callback, "old plugin callback did not drain after reload");
        Checks.expect(ReloadIdentityPlugin.failure.get() == null,
            "old plugin callback failed: " + ReloadIdentityPlugin.failure.get());
        Checks.expect(foton.PluginHost.byName("ReloadIdentity") == replacement
                && pluginLoader("ReloadIdentity") == replacementLoader,
            "old deferred teardown removed the replacement plugin loader");
        try (java.io.InputStream resource =
                replacementLoader.getResourceAsStream("plugin.yml")) {
            Checks.expect(resource != null,
                "old deferred teardown closed the replacement plugin loader");
        }
        foton.PluginHost.disable(replacement);
        discard("ReloadIdentity");
    }

    private static void eventBridgeRace(org.bukkit.plugin.Plugin owner) throws Exception {
        Checks.expect(owner != null && owner.isEnabled(),
            "the event race needs the enabled fixture plugin");
        int baseline = foton.EventBridge.handlerCount(
            org.bukkit.event.player.AsyncPlayerChatEvent.class.getName());
        java.util.concurrent.CountDownLatch start = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        java.util.List<Thread> threads = new java.util.ArrayList<>();

        for (int worker = 0; worker < 4; worker++) {
            final boolean registers = worker < 2;
            Thread thread = new Thread(() -> {
                try {
                    start.await();
                    for (int iteration = 0; iteration < 4_000; iteration++) {
                        if (registers) {
                            org.bukkit.event.Listener listener =
                                new org.bukkit.event.Listener() {};
                            foton.EventBridge.register(listener,
                                org.bukkit.event.player.AsyncPlayerChatEvent.class,
                                org.bukkit.event.EventPriority.NORMAL,
                                (registered, event) -> {}, owner);
                            foton.EventBridge.unregister(listener);
                        } else {
                            foton.EventBridge.dispatch(
                                new org.bukkit.event.player.AsyncPlayerChatEvent(null, "race"));
                        }
                    }
                } catch (Throwable error) {
                    failure.compareAndSet(null, error);
                }
            }, "event-bridge-race-" + worker);
            threads.add(thread);
            thread.start();
        }
        start.countDown();
        for (Thread thread : threads) thread.join();

        Checks.expect(failure.get() == null,
            "concurrent event registration/dispatch failed: " + failure.get());
        Checks.same(foton.EventBridge.handlerCount(
                org.bukkit.event.player.AsyncPlayerChatEvent.class.getName()), baseline,
            "the event race leaked handlers");
    }

    private static void discard(String name) throws Exception {
        discardPlugin(foton.PluginHost.byName(name));
    }

    private static void discardPlugin(org.bukkit.plugin.Plugin plugin) throws Exception {
        if (plugin == null) return;
        java.lang.reflect.Method discard = foton.PluginHost.class.getDeclaredMethod(
            "discard", org.bukkit.plugin.Plugin.class);
        discard.setAccessible(true);
        try {
            discard.invoke(null, plugin);
        } catch (java.lang.reflect.InvocationTargetException error) {
            Throwable cause = error.getCause();
            if (cause instanceof Exception checked) throw checked;
            if (cause instanceof Error unchecked) throw unchecked;
            throw error;
        }
    }

    @SuppressWarnings("unchecked")
    private static org.bukkit.plugin.java.PluginClassLoader pluginLoader(String name)
            throws Exception {
        java.lang.reflect.Field lifecycle = foton.PluginHost.class.getDeclaredField("lifecycle");
        java.lang.reflect.Field loaders =
            foton.PluginHost.class.getDeclaredField("pluginLoaders");
        lifecycle.setAccessible(true);
        loaders.setAccessible(true);
        Object monitor = lifecycle.get(null);
        synchronized (monitor) {
            java.util.Map<String, org.bukkit.plugin.java.PluginClassLoader> current =
                (java.util.Map<String, org.bukkit.plugin.java.PluginClassLoader>) loaders.get(null);
            return current.get(name.toLowerCase(java.util.Locale.ROOT));
        }
    }

    private static void eventBridgeClearRace(org.bukkit.plugin.Plugin owner) throws Exception {
        java.util.concurrent.CountDownLatch start = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        java.util.List<Thread> threads = new java.util.ArrayList<>();

        for (int worker = 0; worker < 4; worker++) {
            final boolean registers = worker < 2;
            Thread thread = new Thread(() -> {
                try {
                    start.await();
                    for (int iteration = 0; iteration < 4_000; iteration++) {
                        if (registers) {
                            foton.EventBridge.register(new org.bukkit.event.Listener() {},
                                org.bukkit.event.player.AsyncPlayerChatEvent.class,
                                org.bukkit.event.EventPriority.NORMAL,
                                (registered, event) -> {}, owner);
                        } else {
                            org.bukkit.event.HandlerList.unregisterAll();
                        }
                    }
                } catch (Throwable error) {
                    failure.compareAndSet(null, error);
                }
            }, "event-bridge-clear-race-" + worker);
            threads.add(thread);
            thread.start();
        }
        start.countDown();
        for (Thread thread : threads) thread.join();
        org.bukkit.event.HandlerList.unregisterAll();

        Checks.expect(failure.get() == null,
            "concurrent event registration/global teardown failed: " + failure.get());
    }

    private static void pluginJar(java.nio.file.Path directory, String name,
            Class<? extends org.bukkit.plugin.java.JavaPlugin> main, String extra)
            throws Exception {
        java.nio.file.Files.createDirectories(directory);
        String descriptor = "name: " + name + "\nversion: 1.0\nmain: "
            + main.getName() + "\n" + extra;
        try (java.util.jar.JarOutputStream jar = new java.util.jar.JarOutputStream(
                 java.nio.file.Files.newOutputStream(directory.resolve(name + ".jar")))) {
            jar.putNextEntry(new java.util.jar.JarEntry("plugin.yml"));
            jar.write(descriptor.getBytes(java.nio.charset.StandardCharsets.UTF_8));
            jar.closeEntry();
        }
    }

    public static final class TeardownPlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        static int disableCalls;
        static int runs;
        static boolean enabledDuringDisable;
        static boolean syncCancelledDuringDisable;
        static boolean rejectedTaskDuringDisable;
        static int handlersDuringDisable;
        static int handlerBaseline;
        static org.bukkit.scheduler.BukkitTask sync;
        static org.bukkit.scheduler.BukkitTask async;

        static void reset() {
            disableCalls = 0;
            runs = 0;
            rejectedTaskDuringDisable = false;
            handlerBaseline = foton.EventBridge.handlerCount(
                org.bukkit.event.player.AsyncPlayerChatEvent.class.getName());
        }

        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
            sync = getServer().getScheduler().runTaskTimer(this, () -> runs++, 100, 1);
            async = getServer().getScheduler().runTaskLaterAsynchronously(
                this, () -> runs++, 100);
        }

        @Override public void onDisable() {
            disableCalls++;
            enabledDuringDisable = isEnabled();
            syncCancelledDuringDisable = sync.isCancelled();
            handlersDuringDisable = foton.EventBridge.handlerCount(
                org.bukkit.event.player.AsyncPlayerChatEvent.class.getName()) - handlerBaseline;
            try {
                getServer().getScheduler().runTask(this, () -> runs++);
            } catch (IllegalStateException expected) {
                rejectedTaskDuringDisable = true;
            }
        }

        @org.bukkit.event.EventHandler
        public void chat(org.bukkit.event.player.AsyncPlayerChatEvent event) {}
    }

    public static final class FailingProvider extends org.bukkit.plugin.java.JavaPlugin {
        static int enableCalls;
        static int disableCalls;
        static void reset() { enableCalls = 0; disableCalls = 0; }
        @Override public void onEnable() {
            enableCalls++;
            throw new IllegalStateException("hostile provider failure");
        }
        @Override public void onDisable() { disableCalls++; }
    }

    public static final class DependentPlugin extends org.bukkit.plugin.java.JavaPlugin {
        static int enableCalls;
        static int disableCalls;
        static void reset() { enableCalls = 0; disableCalls = 0; }
        @Override public void onEnable() { enableCalls++; }
        @Override public void onDisable() { disableCalls++; }
    }

    private static final java.util.List<String> LOAD_ORDER =
        java.util.Collections.synchronizedList(new java.util.ArrayList<>());

    private static final java.util.List<String> SOFT_CYCLE_LOAD_ORDER =
        java.util.Collections.synchronizedList(new java.util.ArrayList<>());

    public static final class BeforePlugin extends org.bukkit.plugin.java.JavaPlugin {
        @Override public void onLoad() { LOAD_ORDER.add("before-load"); }
        @Override public void onEnable() { LOAD_ORDER.add("before-enable"); }
    }

    public static final class TargetPlugin extends org.bukkit.plugin.java.JavaPlugin {
        @Override public void onLoad() { LOAD_ORDER.add("target-load"); }
        @Override public void onEnable() { LOAD_ORDER.add("target-enable"); }
    }

    public static final class SoftCycleAlpha extends org.bukkit.plugin.java.JavaPlugin {
        @Override public void onLoad() { SOFT_CYCLE_LOAD_ORDER.add("alpha"); }
    }

    public static final class SoftCycleBeta extends org.bukkit.plugin.java.JavaPlugin {
        @Override public void onLoad() { SOFT_CYCLE_LOAD_ORDER.add("beta"); }
    }

    public static final class HardAfterSoftCycle extends org.bukkit.plugin.java.JavaPlugin {
        @Override public void onLoad() { SOFT_CYCLE_LOAD_ORDER.add("hard"); }
    }

    public static final class PhaseAlpha extends org.bukkit.plugin.java.JavaPlugin {
        static boolean sawZulu;
        static boolean enabledDuringLoad;
        @Override public void onLoad() {
            org.bukkit.plugin.Plugin zulu = foton.PluginHost.byName("PhaseZulu");
            sawZulu = zulu != null;
            enabledDuringLoad = isEnabled() || (zulu != null && zulu.isEnabled());
        }
    }

    public static final class PhaseZulu extends org.bukkit.plugin.java.JavaPlugin {}

    public static final class FailingOnLoad extends org.bukkit.plugin.java.JavaPlugin {
        static int disableCalls;
        @Override public void onLoad() {
            throw new IllegalStateException("hostile onLoad failure");
        }
        @Override public void onDisable() { disableCalls++; }
    }

    public static final class DependentOnLoad extends org.bukkit.plugin.java.JavaPlugin {
        static int loadCalls;
        @Override public void onLoad() { loadCalls++; }
    }

    public static final class LifecycleEvent extends org.bukkit.event.Event {
        private static final org.bukkit.event.HandlerList HANDLERS =
            new org.bukkit.event.HandlerList();
        @Override public org.bukkit.event.HandlerList getHandlers() { return HANDLERS; }
        public static org.bukkit.event.HandlerList getHandlerList() { return HANDLERS; }
    }

    public static final class CrossDisableAlphaEvent extends org.bukkit.event.Event {
        private static final org.bukkit.event.HandlerList HANDLERS =
            new org.bukkit.event.HandlerList();
        @Override public org.bukkit.event.HandlerList getHandlers() { return HANDLERS; }
        public static org.bukkit.event.HandlerList getHandlerList() { return HANDLERS; }
    }

    public static final class CrossDisableBetaEvent extends org.bukkit.event.Event {
        private static final org.bukkit.event.HandlerList HANDLERS =
            new org.bukkit.event.HandlerList();
        @Override public org.bukkit.event.HandlerList getHandlers() { return HANDLERS; }
        public static org.bukkit.event.HandlerList getHandlerList() { return HANDLERS; }
    }

    public static final class InvocationBarrierPlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        static java.util.concurrent.CountDownLatch entered;
        static java.util.concurrent.CountDownLatch release;
        static java.util.concurrent.atomic.AtomicInteger calls;
        static volatile boolean reentrantLifecycleReturned;
        static void reset() {
            entered = new java.util.concurrent.CountDownLatch(1);
            release = new java.util.concurrent.CountDownLatch(1);
            calls = new java.util.concurrent.atomic.AtomicInteger();
            reentrantLifecycleReturned = false;
        }
        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }
        @org.bukkit.event.EventHandler
        public void onLifecycle(LifecycleEvent event) throws Exception {
            calls.incrementAndGet();
            entered.countDown();
            release.await();
            foton.PluginHost.disable(foton.PluginHost.byName("InvocationCompanion"));
            reentrantLifecycleReturned = true;
        }
    }

    public static final class InvocationCompanionPlugin
            extends org.bukkit.plugin.java.JavaPlugin {}

    public static final class SchedulerBarrierPlugin extends org.bukkit.plugin.java.JavaPlugin {
        static java.util.concurrent.CountDownLatch entered;
        static java.util.concurrent.CountDownLatch release;
        static java.util.concurrent.atomic.AtomicInteger asyncRuns;
        static java.util.concurrent.atomic.AtomicInteger queuedRuns;
        static void reset() {
            entered = new java.util.concurrent.CountDownLatch(1);
            release = new java.util.concurrent.CountDownLatch(1);
            asyncRuns = new java.util.concurrent.atomic.AtomicInteger();
            queuedRuns = new java.util.concurrent.atomic.AtomicInteger();
        }
        @Override public void onEnable() {
            getServer().getScheduler().runTask(this, queuedRuns::incrementAndGet);
            getServer().getScheduler().runTaskAsynchronously(this, () -> {
                entered.countDown();
                try {
                    release.await();
                    asyncRuns.incrementAndGet();
                } catch (InterruptedException error) {
                    Thread.currentThread().interrupt();
                }
            });
        }
    }

    public static final class SelfDisablePlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        static volatile boolean returned;
        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }
        @org.bukkit.event.EventHandler
        public void onLifecycle(LifecycleEvent event) {
            foton.PluginHost.disable(this);
            returned = true;
        }
    }

    private static final class CrossDisable {
        static java.util.concurrent.CountDownLatch entered;
        static java.util.concurrent.atomic.AtomicInteger returned;

        static void reset() {
            entered = new java.util.concurrent.CountDownLatch(2);
            returned = new java.util.concurrent.atomic.AtomicInteger();
        }

        static void disableOther(String name) {
            entered.countDown();
            try {
                if (!entered.await(5, java.util.concurrent.TimeUnit.SECONDS)) {
                    throw new IllegalStateException("the other callback did not enter");
                }
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException(error);
            }
            foton.PluginHost.disable(foton.PluginHost.byName(name));
            returned.incrementAndGet();
        }
    }

    public static final class CrossDisableAlpha extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }
        @org.bukkit.event.EventHandler
        public void onCrossDisable(CrossDisableAlphaEvent event) {
            CrossDisable.disableOther("CrossDisableBeta");
        }
    }

    public static final class CrossDisableBeta extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }
        @org.bukkit.event.EventHandler
        public void onCrossDisable(CrossDisableBetaEvent event) {
            CrossDisable.disableOther("CrossDisableAlpha");
        }
    }

    public static final class LifecycleHelperPlugin extends org.bukkit.plugin.java.JavaPlugin {
        static volatile boolean helperReturned;
        static void reset() { helperReturned = false; }

        @Override public void onDisable() {
            Thread helper = new Thread(() -> foton.PluginHost.disable(
                foton.PluginHost.byName("LifecycleHelperCompanion")),
                "plugin-lifecycle-helper");
            helper.start();
            try {
                helper.join(java.util.concurrent.TimeUnit.SECONDS.toMillis(5));
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException(error);
            }
            helperReturned = !helper.isAlive();
        }
    }

    public static final class LifecycleHelperCompanion
            extends org.bukkit.plugin.java.JavaPlugin {}

    public static final class StoppingObserver extends org.bukkit.plugin.java.JavaPlugin {
        static boolean sawStopping;
        @Override public void onDisable() { sawStopping = org.bukkit.Bukkit.isStopping(); }
    }

    public static final class DisableAllBarrierPlugin extends org.bukkit.plugin.java.JavaPlugin {
        static java.util.concurrent.CountDownLatch entered;
        static java.util.concurrent.CountDownLatch release;
        static volatile boolean resourceAccessible;

        static void reset() {
            entered = new java.util.concurrent.CountDownLatch(1);
            release = new java.util.concurrent.CountDownLatch(1);
            resourceAccessible = false;
        }

        @Override public void onDisable() {
            entered.countDown();
            try {
                release.await();
                try (java.io.InputStream resource = getResource("plugin.yml")) {
                    resourceAccessible = resource != null;
                }
            } catch (InterruptedException error) {
                Thread.currentThread().interrupt();
                throw new IllegalStateException(error);
            } catch (java.io.IOException error) {
                throw new IllegalStateException(error);
            }
        }
    }

    public static final class ConcurrentLifecyclePlugin
            extends org.bukkit.plugin.java.JavaPlugin {
        static java.util.concurrent.atomic.AtomicInteger loadCalls;
        static java.util.concurrent.atomic.AtomicInteger enableCalls;
        static java.util.concurrent.atomic.AtomicInteger disableCalls;
        static volatile boolean reentrantEnableReturned;
        static void reset() {
            loadCalls = new java.util.concurrent.atomic.AtomicInteger();
            enableCalls = new java.util.concurrent.atomic.AtomicInteger();
            disableCalls = new java.util.concurrent.atomic.AtomicInteger();
            reentrantEnableReturned = false;
        }
        @Override public void onLoad() { loadCalls.incrementAndGet(); }
        @Override public void onEnable() { enableCalls.incrementAndGet(); }
        @Override public void onDisable() {
            disableCalls.incrementAndGet();
            foton.PluginHost.enableAll();
            reentrantEnableReturned = true;
        }
    }

    public static final class DisableObserverPlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        static boolean sawEvent;
        static boolean sawPublished;
        static boolean sawCommand;
        static boolean sawHandler;
        static boolean sawLiveLoader;

        static void reset() {
            sawEvent = false;
            sawPublished = false;
            sawCommand = false;
            sawHandler = false;
            sawLiveLoader = false;
        }

        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }

        @org.bukkit.event.EventHandler
        public void onDisable(org.bukkit.event.server.PluginDisableEvent event) {
            if (!event.getPlugin().getName().equals("DisableTarget")) return;
            sawEvent = true;
            sawPublished = foton.PluginHost.byName("DisableTarget") == event.getPlugin();
            sawCommand = foton.CommandMap.get("visible-during-disable") != null;
            sawHandler = foton.EventBridge.handlerCount(LifecycleEvent.class.getName()) > 0;
            try {
                org.bukkit.plugin.java.PluginClassLoader loader =
                    pluginLoader("DisableTarget");
                try (java.io.InputStream resource = loader == null ? null
                        : loader.getResourceAsStream("plugin.yml")) {
                    sawLiveLoader = resource != null;
                }
            } catch (Exception error) {
                throw new IllegalStateException(error);
            }
        }
    }

    public static final class DisableTargetPlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }

        @org.bukkit.event.EventHandler
        public void onLifecycle(LifecycleEvent event) {}
    }

    public static final class ReloadIdentityPlugin extends org.bukkit.plugin.java.JavaPlugin
            implements org.bukkit.event.Listener {
        static java.util.concurrent.CountDownLatch discarded;
        static java.util.concurrent.CountDownLatch release;
        static java.util.concurrent.atomic.AtomicReference<Throwable> failure;

        static void reset() {
            discarded = new java.util.concurrent.CountDownLatch(1);
            release = new java.util.concurrent.CountDownLatch(1);
            failure = new java.util.concurrent.atomic.AtomicReference<>();
        }

        @Override public void onEnable() {
            getServer().getPluginManager().registerEvents(this, this);
        }

        @org.bukkit.event.EventHandler
        public void onLifecycle(LifecycleEvent event) {
            try {
                discardPlugin(this);
                discarded.countDown();
                release.await();
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
                discarded.countDown();
            }
        }
    }
}
