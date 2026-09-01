package org.bukkit.command;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** A command, as Bukkit describes one. */
public abstract class Command {
    private final String name;
    private String description = "";
    private String usage = "";
    private String permission;
    private String permissionMessage;
    private List<String> aliases = new ArrayList<>();

    protected Command(String name) {
        this.name = name;
    }

    public String getName() {
        return name;
    }

    public String getLabel() {
        return name;
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

    public abstract boolean execute(CommandSender sender, String label, String[] args);

    public List<String> tabComplete(CommandSender sender, String label, String[] args) {
        return List.of();
    }
}
