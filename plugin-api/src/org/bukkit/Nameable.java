package org.bukkit;

import org.jetbrains.annotations.Nullable;

/** Something carrying an optional custom display name. */
public interface Nameable {
    @Nullable net.kyori.adventure.text.Component customName();
    void customName(@Nullable net.kyori.adventure.text.Component name);
    @Nullable String getCustomName();
    void setCustomName(@Nullable String name);
}
