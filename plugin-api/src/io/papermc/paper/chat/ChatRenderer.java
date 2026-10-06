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

    /** Vanilla's {@code chat.type.text}: {@code <name> message}. */
    static ChatRenderer defaultRenderer() {
        return DefaultRenderer.INSTANCE;
    }

    static ChatRenderer viewerUnaware(ViewerUnaware renderer) {
        if (renderer == null) return null;
        return (source, sourceDisplayName, message, viewer) -> renderer.render(source, sourceDisplayName, message);
    }

    /** The renderer a chat event starts with; the server knows it by identity. */
    final class DefaultRenderer implements ChatRenderer {
        static final DefaultRenderer INSTANCE = new DefaultRenderer();

        private DefaultRenderer() { }

        @Override
        public Component render(Player source, Component sourceDisplayName, Component message, Audience viewer) {
            return Component.translatable("chat.type.text", sourceDisplayName, message);
        }
    }
}
