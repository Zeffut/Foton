package org.bukkit.help;
import org.bukkit.command.CommandSender;
public abstract class HelpTopic {
    public abstract String getName();
    public abstract String getShortText();
    public String getFullText(CommandSender sender) { return getShortText(); }
    public boolean canSee(CommandSender sender) { return true; }
}
