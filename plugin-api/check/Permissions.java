/** Canonical player handles and permission attachment lifecycle. */
final class Permissions {
    private Permissions() {}

    static void check() throws Exception {
        org.bukkit.plugin.Plugin owner = foton.PluginHost.byName("EventFixture");
        Checks.expect(owner != null && owner.isEnabled(),
            "the permission check needs the enabled fixture plugin");

        java.util.UUID id = java.util.UUID.fromString(
            "00000000-0000-0000-0000-000000000042");
        foton.EventBridge.fireJoin(id.toString(), "");
        foton.FotonPlayer first = new foton.FotonPlayer(id);
        foton.FotonPlayer second = new foton.FotonPlayer(id);

        org.bukkit.permissions.PermissionAttachment fallback =
            first.addAttachment(new Owner("fallback"));
        fallback.setPermission("fixture.shared", false);
        org.bukkit.permissions.PermissionAttachment owned = first.addAttachment(owner);
        owned.setPermission("FiXtUrE.ShArEd", true);

        Checks.expect(first.hasPermission("fixture.shared")
                && second.hasPermission("FIXTURE.SHARED"),
            "two handles for one UUID did not share their attachments");

        owned.setPermission("fixture.shared", false);
        second.recalculatePermissions();
        Checks.expect(!first.hasPermission("fixture.shared"),
            "recalculatePermissions did not rebuild the canonical result");

        owned.setPermission("fixture.shared", true);
        owned.remove();
        Checks.expect(!second.hasPermission("fixture.shared"),
            "PermissionAttachment.remove did not reveal the previous value");

        permissionKeysUseLocaleRoot(first, owner);
        abortedDuplicateDoesNotInvalidateIncumbent(owner);

        quitPurgesSessionAttachmentsAfterTheEvent(owner);
        concurrentLastRemovalCannotLoseANewAttachment(owner);

        org.bukkit.permissions.PermissionAttachment disabled = second.addAttachment(owner);
        disabled.setPermission("fixture.shared", true);
        concurrentDisableCannotLeakAttachments(owner);
        Checks.expect(!first.hasPermission("fixture.shared")
                && !second.hasPermission("fixture.shared"),
            "disabling a plugin left a player permission attachment active");
        boolean rejected = false;
        try {
            second.addAttachment(owner);
        } catch (IllegalStateException expected) {
            rejected = true;
        }
        Checks.expect(rejected,
            "a disabled plugin was allowed to recreate an attachment after teardown");
    }

    private static void concurrentDisableCannotLeakAttachments(
            org.bukkit.plugin.Plugin owner) throws Exception {
        final int workers = 4;
        final int attempts = 10_000;
        java.util.concurrent.CountDownLatch start = new java.util.concurrent.CountDownLatch(1);
        java.util.concurrent.CountDownLatch active =
            new java.util.concurrent.CountDownLatch(workers);
        java.util.concurrent.ConcurrentLinkedQueue<java.util.UUID> attached =
            new java.util.concurrent.ConcurrentLinkedQueue<>();
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        java.util.List<Thread> threads = new java.util.ArrayList<>();

        for (int worker = 0; worker < workers; worker++) {
            final int workerId = worker;
            java.util.UUID workerUuid = new java.util.UUID(workerId + 1L, 1L);
            foton.EventBridge.fireJoin(workerUuid.toString(), "");
            Thread thread = new Thread(() -> {
                try {
                    start.await();
                    for (int attempt = 0; attempt < attempts; attempt++) {
                        java.util.UUID id = workerUuid;
                        try {
                            org.bukkit.permissions.PermissionAttachment attachment =
                                new foton.FotonPlayer(id).addAttachment(owner);
                            attachment.setPermission("fixture.disable-race", true);
                            attached.add(id);
                            if (attempt == 0) active.countDown();
                        } catch (IllegalStateException disabled) {
                            active.countDown();
                            break;
                        }
                    }
                } catch (Throwable error) {
                    failure.compareAndSet(null, error);
                    active.countDown();
                }
            }, "permission-disable-race-" + worker);
            threads.add(thread);
            thread.start();
        }

        start.countDown();
        active.await();
        foton.PluginHost.disable(owner);
        for (Thread thread : threads) thread.join();
        Checks.expect(failure.get() == null,
            "the permission/disable race failed: " + failure.get());

        for (java.util.UUID id : attached) {
            Checks.expect(!foton.FotonPlayer.hasAttachmentState(id),
                "a concurrent permission attachment survived plugin disable for " + id);
        }
    }

    private static void quitPurgesSessionAttachmentsAfterTheEvent(
            org.bukkit.plugin.Plugin owner) throws Exception {
        java.util.UUID id = java.util.UUID.fromString(
            "00000000-0000-0000-0000-000000000043");
        foton.EventBridge.fireJoin(id.toString(), "");
        foton.FotonPlayer player = new foton.FotonPlayer(id);
        org.bukkit.permissions.PermissionAttachment attachment = player.addAttachment(owner);
        attachment.setPermission("fixture.quit", true);

        java.util.concurrent.atomic.AtomicBoolean visibleDuringQuit =
            new java.util.concurrent.atomic.AtomicBoolean();
        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() {};
        foton.EventBridge.register(listener, org.bukkit.event.player.PlayerQuitEvent.class,
            org.bukkit.event.EventPriority.NORMAL,
            (registered, event) -> visibleDuringQuit.set(
                ((org.bukkit.event.player.PlayerQuitEvent) event).getPlayer()
                    .hasPermission("fixture.quit")), owner);
        foton.EventBridge.fireQuit(id.toString(), "bye");
        foton.EventBridge.unregister(listener);

        Checks.expect(visibleDuringQuit.get(),
            "quit listeners lost session attachments before PlayerQuitEvent");
        Checks.expect(!foton.FotonPlayer.hasAttachmentState(id),
            "a disconnected player kept a session attachment after PlayerQuitEvent");
        boolean staleRejected = false;
        try {
            player.addAttachment(owner);
        } catch (IllegalStateException expected) {
            staleRejected = true;
        }
        Checks.expect(staleRejected,
            "a player handle from a closed session recreated attachments");

        foton.EventBridge.fireJoin(id.toString(), "");
        foton.FotonPlayer replacement = new foton.FotonPlayer(id);
        replacement.addAttachment(owner).setPermission("fixture.new-session", true);
        Checks.expect(replacement.hasPermission("fixture.new-session"),
            "a replacement player session could not create attachments");
        try {
            player.addAttachment(owner);
            throw new AssertionError("the old handle joined the replacement session");
        } catch (IllegalStateException expected) { }
    }

    private static void concurrentLastRemovalCannotLoseANewAttachment(
            org.bukkit.plugin.Plugin owner) throws Exception {
        java.util.UUID id = java.util.UUID.fromString(
            "00000000-0000-0000-0000-000000000044");
        foton.EventBridge.fireJoin(id.toString(), "");
        foton.FotonPlayer player = new foton.FotonPlayer(id);
        java.util.concurrent.atomic.AtomicReference<org.bukkit.permissions.PermissionAttachment>
            previous = new java.util.concurrent.atomic.AtomicReference<>(player.addAttachment(owner));
        java.util.concurrent.atomic.AtomicReference<org.bukkit.permissions.PermissionAttachment>
            next = new java.util.concurrent.atomic.AtomicReference<>();
        java.util.concurrent.atomic.AtomicReference<Throwable> failure =
            new java.util.concurrent.atomic.AtomicReference<>();
        java.util.concurrent.CyclicBarrier start = new java.util.concurrent.CyclicBarrier(3);
        java.util.concurrent.CyclicBarrier done = new java.util.concurrent.CyclicBarrier(3);
        final int iterations = 4_000;

        Thread remover = new Thread(() -> {
            try {
                for (int iteration = 0; iteration < iterations; iteration++) {
                    start.await();
                    player.removeAttachment(previous.get());
                    done.await();
                }
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        }, "permission-remove-race");
        Thread adder = new Thread(() -> {
            try {
                for (int iteration = 0; iteration < iterations; iteration++) {
                    start.await();
                    org.bukkit.permissions.PermissionAttachment attachment =
                        player.addAttachment(owner);
                    attachment.setPermission("fixture.concurrent", true);
                    next.set(attachment);
                    done.await();
                }
            } catch (Throwable error) {
                failure.compareAndSet(null, error);
            }
        }, "permission-add-race");
        remover.start();
        adder.start();

        for (int iteration = 0; iteration < iterations; iteration++) {
            start.await();
            done.await();
            Checks.expect(failure.get() == null,
                "the permission attachment race failed: " + failure.get());
            Checks.expect(player.hasPermission("fixture.concurrent"),
                "removing the previous last attachment lost a concurrent new attachment");
            previous.set(next.get());
        }
        remover.join();
        adder.join();
        player.removeAttachment(previous.get());
    }

    private static void permissionKeysUseLocaleRoot(foton.FotonPlayer player,
            org.bukkit.plugin.Plugin owner) {
        java.util.Locale previous = java.util.Locale.getDefault();
        try {
            java.util.Locale.setDefault(java.util.Locale.forLanguageTag("tr-TR"));
            org.bukkit.permissions.PermissionAttachment attachment = player.addAttachment(owner);
            attachment.setPermission("FIXTURE.I", true);
            Checks.expect(attachment.getPermissions().containsKey("fixture.i")
                    && player.hasPermission("FiXtUrE.I"),
                "permission keys were normalized with the process locale");
            attachment.unsetPermission("fIxTuRe.I");
            Checks.expect(!attachment.getPermissions().containsKey("fixture.i"),
                "mixed-case unset did not remove the normalized permission key");
            attachment.remove();
        } finally {
            java.util.Locale.setDefault(previous);
        }
    }

    private static void abortedDuplicateDoesNotInvalidateIncumbent(
            org.bukkit.plugin.Plugin owner) {
        java.util.UUID id = java.util.UUID.fromString(
            "00000000-0000-0000-0000-000000000045");
        foton.EventBridge.fireJoin(id.toString(), 1, "");
        foton.FotonPlayer incumbent = new foton.FotonPlayer(id);
        org.bukkit.permissions.PermissionAttachment incumbentPermissions =
            incumbent.addAttachment(owner);
        incumbentPermissions.setPermission("fixture.incumbent", true);
        incumbentPermissions.setPermission("fixture.candidate", false);

        org.bukkit.event.Listener listener = new org.bukkit.event.Listener() {};
        foton.EventBridge.register(listener, org.bukkit.event.player.PlayerLoginEvent.class,
            org.bukkit.event.EventPriority.NORMAL, (registered, event) -> {
                org.bukkit.entity.Player candidate =
                    ((org.bukkit.event.player.PlayerLoginEvent) event).getPlayer();
                candidate.addAttachment(owner).setPermission("fixture.candidate", true);
            }, owner);
        org.bukkit.event.player.PlayerLoginEvent duplicate =
            new org.bukkit.event.player.PlayerLoginEvent(
                beginLoginAttempt(id, 2), null, null);
        foton.EventBridge.dispatch(duplicate);
        Checks.expect(!duplicate.isCancelled(),
            "duplicate login fixture was unexpectedly denied");
        Checks.expect(incumbent.hasPermission("fixture.incumbent")
                && !incumbent.hasPermission("fixture.candidate"),
            "pending duplicate session replaced or leaked into the incumbent");

        foton.EventBridge.abortLogin(id.toString(), 2);
        Checks.expect(incumbent.hasPermission("fixture.incumbent"),
            "aborting a duplicate login invalidated the incumbent session");
        foton.EventBridge.unregister(listener);

        beginLoginAttempt(id, 3);
        foton.EventBridge.fireJoin(id.toString(), 3, "");
        boolean staleRejected = false;
        try {
            incumbent.addAttachment(owner);
        } catch (IllegalStateException expected) {
            staleRejected = true;
        }
        Checks.expect(staleRejected,
            "committing the replacement did not close the incumbent handle");
        foton.EventBridge.fireQuit(id.toString(), "");
    }

    private static org.bukkit.entity.Player beginLoginAttempt(java.util.UUID id, int attemptId) {
        try {
            java.lang.reflect.Method begin = foton.FotonPlayer.class.getDeclaredMethod(
                "beginLoginAttempt", String.class, int.class);
            begin.setAccessible(true);
            return (org.bukkit.entity.Player) begin.invoke(null, id.toString(), attemptId);
        } catch (ReflectiveOperationException error) {
            throw new AssertionError("could not begin a login attempt", error);
        }
    }

    private static final class Owner implements org.bukkit.plugin.Plugin {
        private final String name;

        private Owner(String name) { this.name = name; }
        @Override public java.io.File getDataFolder() { return null; }
        @Override public org.bukkit.plugin.PluginDescriptionFile getDescription() { return null; }
        @Override public org.bukkit.Server getServer() { return org.bukkit.Bukkit.getServer(); }
        @Override public java.util.logging.Logger getLogger() { return null; }
        @Override public String getName() { return name; }
        @Override public boolean isEnabled() { return true; }
        @Override public void onEnable() {}
        @Override public void onDisable() {}
    }
}
