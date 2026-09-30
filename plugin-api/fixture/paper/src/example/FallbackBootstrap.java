package example;

import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;

public final class FallbackBootstrap implements PluginBootstrap {
    @Override
    public void bootstrap(BootstrapContext context) {
        System.setProperty("foton.fixture.paper.fallback.bootstrap", "true");
    }
}
