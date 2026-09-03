package org.bukkit.help;

import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

/** Default help topic backed by a Bukkit command. */
public class GenericCommandHelpTopic extends HelpTopic {
    private final Command command;
    private final String name;
    private final String shortText;

    public GenericCommandHelpTopic(Command command) {
        if (command == null) throw new IllegalArgumentException("command");
        this.command = command;
        this.name = "/" + command.getName();
        this.shortText = command.getDescription();
    }

    @Override public String getName() { return name; }
    @Override public String getShortText() { return shortText; }

    @Override public String getFullText(CommandSender sender) {
        String usage = command.getUsage();
        return usage == null || usage.isEmpty() ? shortText : shortText + " Usage: " + usage;
    }

    @Override public boolean canSee(CommandSender sender) {
        return command.testPermissionSilent(sender);
    }
}
