package example;

public final class StaticInitializerBootstrap {
    static {
        System.setProperty("foton.fixture.paper.invalid.staticInitializer", "ran");
    }
}
