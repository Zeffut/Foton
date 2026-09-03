package io.papermc.paper.datapack;

import java.util.Collection;

/** Access to datapacks discovered in the active Foton datapack directory. */
public interface DatapackManager {
    Collection<Datapack> getPacks();
    Collection<Datapack> getEnabledPacks();
}
