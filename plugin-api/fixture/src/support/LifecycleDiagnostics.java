package foton;

/** Test-only access to package-private lifecycle retention diagnostics. */
public final class LifecycleDiagnostics {
    private LifecycleDiagnostics() {}

    public static int schedulerTasks(String plugin) {
        return FotonScheduler.trackedTaskCount(plugin);
    }

    public static boolean schedulerAccepts(String plugin) {
        return FotonScheduler.acceptsTasks(plugin);
    }

    public static int hostReferences(String plugin) {
        return PluginHost.lifecycleReferenceCount(plugin);
    }

    public static int eventTypes(String event) {
        return EventBridge.eventTypeCount(event);
    }
}
