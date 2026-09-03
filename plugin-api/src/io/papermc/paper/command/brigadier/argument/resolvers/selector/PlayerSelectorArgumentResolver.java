package io.papermc.paper.command.brigadier.argument.resolvers.selector;

import io.papermc.paper.command.brigadier.CommandSourceStack;
import org.bukkit.entity.Player;

/** Resolves a player selector against the current Bukkit server. */
public final class PlayerSelectorArgumentResolver {
    private final String name;
    public PlayerSelectorArgumentResolver(String name) { this.name = name; }
    public Object resolve(CommandSourceStack source) {
        if (source == null || name == null) return null;
        return org.bukkit.Bukkit.getPlayerExact(name);
    }
}
