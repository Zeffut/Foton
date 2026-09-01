package org.bukkit.event;

public abstract class Event {
    public String getEventName() { return getClass().getSimpleName(); }
}
