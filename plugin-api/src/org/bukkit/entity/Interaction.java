package org.bukkit.entity;

/** An invisible box that reports the clicks and hits it receives. */
public interface Interaction extends Entity {
    float getInteractionWidth();
    void setInteractionWidth(float width);
    float getInteractionHeight();
    void setInteractionHeight(float height);
    /** Whether a click on the box swings the player's arm, as a real hit would. */
    boolean isResponsive();
    void setResponsive(boolean response);
}
