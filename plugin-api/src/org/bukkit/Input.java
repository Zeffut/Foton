package org.bukkit;

/** The movement keys a player's client last reported holding. */
public interface Input {
    boolean isForward();
    boolean isBackward();
    boolean isLeft();
    boolean isRight();
    boolean isJump();
    boolean isSneak();
    boolean isSprint();
}
