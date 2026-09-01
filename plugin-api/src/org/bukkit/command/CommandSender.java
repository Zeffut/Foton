package org.bukkit.command;

public interface CommandSender {
    void sendMessage(String message);
    boolean hasPermission(String permission);
    String getName();
}
