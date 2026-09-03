package io.papermc.paper.command.brigadier.argument;

import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.arguments.StringArgumentType;

/** Paper argument type factory backed by Brigadier's stable parsers. */
public final class ArgumentTypes {
    private ArgumentTypes() {}
    public static ArgumentType<?> blockPosition() { return StringArgumentType.string(); }
    public static ArgumentType<?> namespacedKey() { return StringArgumentType.word(); }
    public static ArgumentType<?> player() { return StringArgumentType.word(); }
}
