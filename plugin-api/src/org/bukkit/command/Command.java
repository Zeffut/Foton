package org.bukkit.command;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** A command, as Bukkit describes one. */
public abstract class Command {
    private final String name;
    private String label;
    private String description = "";
    private String usage = "";
    private String permission;
    private String permissionMessage;
    private List<String> aliases = new ArrayList<>();

    protected Command(String name) {
        this.name = name;
        this.label = name;
    }
    protected Command(String name, String description, String usage, List<String> aliases) {
        this(name); setDescription(description); setUsage(usage); setAliases(aliases);
    }
    public boolean register(CommandMap map) { return map != null && map.register(getName(), this); }
    public boolean unregister(CommandMap map) { return map instanceof SimpleCommandMap && ((SimpleCommandMap) map).unregister(this); }

    public String getName() {
        return name;
    }

    public String getLabel() {
        return label;
    }

    public boolean setLabel(String value) {
        if (value == null || value.isEmpty()) return false;
        this.label = value;
        return true;
    }

    public String getDescription() {
        return description;
    }

    public Command setDescription(String value) {
        this.description = value == null ? "" : value;
        return this;
    }

    public String getUsage() {
        return usage;
    }

    public Command setUsage(String value) {
        this.usage = value == null ? "" : value;
        return this;
    }

    public String getPermission() {
        return permission;
    }

    public void setPermission(String value) {
        this.permission = value;
    }

    public String getPermissionMessage() {
        return permissionMessage;
    }

    public Command setPermissionMessage(String value) {
        this.permissionMessage = value;
        return this;
    }

    public List<String> getAliases() {
        return Collections.unmodifiableList(aliases);
    }

    public Command setAliases(List<String> value) {
        this.aliases = value == null ? new ArrayList<>() : new ArrayList<>(value);
        return this;
    }

    /** Whether the sender may run this at all. */
    public boolean testPermission(CommandSender sender) {
        if (permission == null || permission.isEmpty() || sender.hasPermission(permission)) {
            return true;
        }
        sender.sendMessage(permissionMessage == null
            ? "You do not have permission to use this command."
            : permissionMessage);
        return false;
    }

    /** Permission check without sending a denial message. */
    public boolean testPermissionSilent(CommandSender sender) {
        return permission == null || permission.isEmpty() || sender.hasPermission(permission);
    }

    public abstract boolean execute(CommandSender sender, String label, String[] args);

    /** Bukkit's default: the online players whose names start with the last
     * argument, among those the sender can see, sorted without regard to case. */
    public List<String> tabComplete(CommandSender sender, String label, String[] args) {
        if (args.length == 0) return List.of();
        String last = args[args.length - 1].toLowerCase(java.util.Locale.ROOT);
        org.bukkit.entity.Player viewer = sender instanceof org.bukkit.entity.Player player ? player : null;
        java.util.ArrayList<String> names = new java.util.ArrayList<>();
        for (org.bukkit.entity.Player player : org.bukkit.Bukkit.getOnlinePlayers()) {
            String name = player.getName();
            if ((viewer == null || viewer.canSee(player))
                    && name.toLowerCase(java.util.Locale.ROOT).startsWith(last)) {
                names.add(name);
            }
        }
        names.sort(String.CASE_INSENSITIVE_ORDER);
        return names;
    }
}
