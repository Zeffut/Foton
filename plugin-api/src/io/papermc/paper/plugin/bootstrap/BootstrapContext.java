package io.papermc.paper.plugin.bootstrap;

import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;

/** Paper bootstrap context exposed to plugins during bootstrap. */
public interface BootstrapContext extends PluginProviderContext,
        io.papermc.paper.plugin.lifecycle.event.LifecycleEventOwner {
    LifecycleEventManager getLifecycleManager();
}
