package fixture.dependencies;

public final class DependencyPlugins {
    private DependencyPlugins() {}

    public static final class AlphaTarget extends DependencyPlugin {}
    public static final class ZuluBefore extends DependencyPlugin {}
    public static final class Provider extends DependencyPlugin {}
    public static final class Consumer extends DependencyPlugin {}
    public static final class BootstrapProvider extends DependencyPlugin {}
    public static final class PaperConsumer extends DependencyPlugin {}
    public static final class ServerAfter extends DependencyPlugin {}
    public static final class NoJoinProvider extends DependencyPlugin {}
    public static final class NoJoinConsumer extends DependencyPlugin {}
    public static final class OptionalA extends DependencyPlugin {}
    public static final class OptionalB extends DependencyPlugin {}
    public static final class Unrelated extends DependencyPlugin {}
    public static final class BrokenRequired extends DependencyPlugin {}
    public static final class BrokenDependent extends DependencyPlugin {}
    public static final class RequiredCycleA extends DependencyPlugin {}
    public static final class RequiredCycleB extends DependencyPlugin {}
    public static final class AliasOne extends DependencyPlugin {}
    public static final class AliasTwo extends DependencyPlugin {}
    public static final class NameOne extends DependencyPlugin {}
    public static final class NameTwo extends DependencyPlugin {}
    public static final class HealthyDuplicate extends DependencyPlugin {}
}
