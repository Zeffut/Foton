package org.bukkit.advancement;

/** Display metadata for an advancement. */
public class AdvancementDisplay {
    private final String title, description;
    private final boolean hidden, announce;
    public AdvancementDisplay(String title, String description, boolean hidden, boolean announce) {
        this.title = title == null ? "" : title; this.description = description == null ? "" : description;
        this.hidden = hidden; this.announce = announce;
    }
    public String getTitle() { return title; }
    public String getDescription() { return description; }
    public boolean isHidden() { return hidden; }
    public boolean shouldAnnounceChat() { return announce; }
}
