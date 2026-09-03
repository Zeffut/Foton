package net.minecraft.commands.arguments.item;

import com.mojang.brigadier.arguments.ArgumentType;
import net.minecraft.commands.CommandBuildContext;

/** Command argument compatibility surface for item predicates. */
public class ItemPredicateArgument implements ArgumentType<Object> {
    public ItemPredicateArgument(CommandBuildContext context) {}
    public static ItemPredicateArgument itemPredicate(CommandBuildContext context) { return new ItemPredicateArgument(context); }
    @Override public Object parse(com.mojang.brigadier.StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException { return null; }
}
