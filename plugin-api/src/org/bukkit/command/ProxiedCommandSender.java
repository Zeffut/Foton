package org.bukkit.command;

/** A command sender acting on behalf of another sender. */
public interface ProxiedCommandSender extends CommandSender {
    CommandSender getCallee();
    CommandSender getCaller();
}
