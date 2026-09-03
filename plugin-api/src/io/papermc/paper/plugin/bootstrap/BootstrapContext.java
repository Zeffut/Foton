package io.papermc.paper.plugin.bootstrap;

import io.papermc.paper.plugin.lifecycle.event.LifecycleEventManager;

/** Paper bootstrap context exposed to plugins during bootstrap. */
public interface BootstrapContext {
    LifecycleEventManager getLifecycleManager();
    default io.papermc.paper.plugin.configuration.PluginMeta getPluginMeta() { return null; }
}
