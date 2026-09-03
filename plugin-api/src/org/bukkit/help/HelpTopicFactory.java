package org.bukkit.help;
import org.bukkit.command.Command;
public interface HelpTopicFactory<T extends Command> { HelpTopic createTopic(T command); }
