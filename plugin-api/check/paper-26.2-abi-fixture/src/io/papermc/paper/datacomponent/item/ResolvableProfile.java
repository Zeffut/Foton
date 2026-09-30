package io.papermc.paper.datacomponent.item;

import io.papermc.paper.datacomponent.DataComponentBuilder;
import java.util.UUID;

/** Minimal Paper 26.2 binary-compatibility fixture. */
public interface ResolvableProfile {
    UUID uuid();
    String name();

    interface Builder extends DataComponentBuilder<ResolvableProfile> {}
}
