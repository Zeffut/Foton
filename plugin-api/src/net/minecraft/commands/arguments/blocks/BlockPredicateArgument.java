package net.minecraft.commands.arguments.blocks;

import com.mojang.brigadier.arguments.ArgumentType;
import net.minecraft.commands.CommandBuildContext;

/** Command argument compatibility surface for block predicates. */
public class BlockPredicateArgument implements ArgumentType<Object> {
    public BlockPredicateArgument(CommandBuildContext context) {}
    public static BlockPredicateArgument blockPredicate(CommandBuildContext context) { return new BlockPredicateArgument(context); }
    @Override public Object parse(com.mojang.brigadier.StringReader reader) throws com.mojang.brigadier.exceptions.CommandSyntaxException { return null; }
}
