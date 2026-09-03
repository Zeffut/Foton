package io.papermc.paper.advancement;

/** Paper naming-compatible advancement display view. */
public class AdvancementDisplay extends org.bukkit.advancement.AdvancementDisplay {
    public AdvancementDisplay(String title, String description, boolean hidden, boolean announce) {
        super(title, description, hidden, announce);
    }
    public boolean doesAnnounceToChat() { return shouldAnnounceChat(); }
    public net.kyori.adventure.text.Component title() { return net.kyori.adventure.text.Component.text(getTitle()); }
}
