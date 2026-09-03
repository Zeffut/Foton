package io.papermc.paper.chat;

import net.kyori.adventure.audience.Audience;
import net.kyori.adventure.text.Component;
import org.bukkit.entity.Player;

/** Paper renderer invoked for each chat recipient. */
@FunctionalInterface
public interface ChatRenderer {
    Component render(Player source, Component sourceDisplayName, Component message, Audience viewer);

    @FunctionalInterface
    interface ViewerUnaware {
        Component render(Player source, Component sourceDisplayName, Component message);
    }

    static ChatRenderer viewerUnaware(ViewerUnaware renderer) {
        if (renderer == null) return null;
        return (source, sourceDisplayName, message, viewer) -> renderer.render(source, sourceDisplayName, message);
    }
}
