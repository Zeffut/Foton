package example;

import io.papermc.paper.command.brigadier.Commands;
import io.papermc.paper.plugin.bootstrap.BootstrapContext;
import io.papermc.paper.plugin.bootstrap.PluginBootstrap;
import io.papermc.paper.plugin.lifecycle.event.types.LifecycleEvents;

public final class FailingBootstrap implements PluginBootstrap {
    @Override
    public void bootstrap(BootstrapContext context) {
        System.setProperty("foton.fixture.paper.failing.bootstrap", "true");
        context.getLifecycleManager().registerEventHandler(LifecycleEvents.COMMANDS, event -> {
            Commands commands = (Commands) event.registrar();
            commands.register(Commands.literal("leakedbootstrap").build());
        });
        throw new IllegalStateException("bootstrap fixture failure");
    }
}
