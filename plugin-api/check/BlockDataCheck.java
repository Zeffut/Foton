import org.bukkit.block.BlockFace;
import org.bukkit.block.data.*;
import org.bukkit.block.data.type.*;

/** BlockData implements every interface the block's properties allow, and its setters write back. */
final class BlockDataCheck {
    private BlockDataCheck() {}

    static void check() {
        BlockData campfire = PropertyBlockData.of(
            "minecraft:campfire[facing=north,lit=true,signal_fire=false,waterlogged=false]");
        Checks.expect(campfire instanceof Campfire && campfire instanceof Lightable
            && campfire instanceof Directional && campfire instanceof Waterlogged,
            "campfire is Lightable + Directional + Waterlogged");
        Checks.expect(!(campfire instanceof Powerable) && !(campfire instanceof Bisected),
            "campfire has no powered or half property");
        ((Lightable) campfire).setLit(false);
        ((Waterlogged) campfire).setWaterlogged(true);
        Checks.same(campfire.getAsString(),
            "minecraft:campfire[facing=north,lit=false,signal_fire=false,waterlogged=true]",
            "campfire setters write back");

        BlockData stairs = PropertyBlockData.of(
            "minecraft:oak_stairs[facing=east,half=top,shape=straight,waterlogged=true]");
        Checks.expect(stairs instanceof Stairs && stairs instanceof Bisected
            && stairs instanceof Directional && stairs instanceof Waterlogged,
            "stairs are Bisected + Directional + Waterlogged");
        Checks.same(((Bisected) stairs).getHalf(), Bisected.Half.TOP, "stairs top half");
        ((Bisected) stairs).setHalf(Bisected.Half.BOTTOM);
        Checks.same(((Directional) stairs).getFacing(), BlockFace.EAST, "stairs facing");
        Checks.same(((Stairs) stairs).getShape(), Stairs.Shape.STRAIGHT, "stairs shape");
        Checks.expect(stairs.getAsString().contains("half=bottom"), "stairs half writes top/bottom");

        BlockData furnace = PropertyBlockData.of("minecraft:furnace[facing=north,lit=false]");
        Checks.expect(furnace instanceof Furnace && furnace instanceof Lightable
            && furnace instanceof Directional && !(furnace instanceof Waterlogged),
            "furnace is Lightable + Directional only");

        BlockData door = PropertyBlockData.of(
            "minecraft:oak_door[facing=north,half=upper,hinge=left,open=false,powered=false]");
        Checks.expect(door instanceof Door && door instanceof Openable && door instanceof Powerable,
            "door is Openable + Powerable");
        Checks.same(((Bisected) door).getHalf(), Bisected.Half.TOP, "door upper half is TOP");
        ((Bisected) door).setHalf(Bisected.Half.BOTTOM);
        ((Openable) door).setOpen(true);
        Checks.same(door.getAsString(),
            "minecraft:oak_door[facing=north,half=lower,hinge=left,open=true,powered=false]",
            "door setters write back");

        BlockData fence = PropertyBlockData.of(
            "minecraft:oak_fence[east=false,north=true,south=false,waterlogged=false,west=false]");
        Checks.expect(fence instanceof MultipleFacing, "fence has independent faces");
        MultipleFacing sides = (MultipleFacing) fence;
        sides.setFace(BlockFace.EAST, true);
        Checks.same(sides.getFaces(), java.util.Set.of(BlockFace.NORTH, BlockFace.EAST), "fence faces");
        Checks.same(sides.getAllowedFaces().size(), 4, "fence allows its four sides");
        Checks.expect(!(PropertyBlockData.of(
            "minecraft:cobblestone_wall[east=none,north=low,south=none,up=true,waterlogged=false,west=none]")
            instanceof MultipleFacing), "a wall names its sides with an enum, not flags");

        BlockData sign = PropertyBlockData.of("minecraft:oak_sign[rotation=4,waterlogged=false]");
        Checks.expect(sign instanceof Sign && sign instanceof Rotatable && sign instanceof Waterlogged,
            "standing sign is Rotatable");
        Checks.same(((Rotatable) sign).getRotation(), BlockFace.WEST, "rotation 4 faces west");
        BlockData wallSign = PropertyBlockData.of("minecraft:oak_wall_sign[facing=north,waterlogged=false]");
        Checks.expect(wallSign instanceof WallSign && !(wallSign instanceof Rotatable),
            "wall sign is Directional, not Rotatable");

        Checks.expect(!(PropertyBlockData.of("minecraft:stone") instanceof Lightable),
            "a block without properties implements no state interface");

        BlockData copy = campfire.clone();
        ((Lightable) copy).setLit(true);
        Checks.expect(!((Lightable) campfire).isLit(), "clone is detached");
        Checks.expect(PropertyBlockData.of(campfire.getAsString()).equals(campfire)
            && PropertyBlockData.of(campfire.getAsString()).hashCode() == campfire.hashCode(),
            "equal states are equal");
        Checks.expect(!copy.equals(campfire), "different states differ");
        Checks.expect(campfire.getMaterial() == org.bukkit.Material.CAMPFIRE, "campfire material");
        Checks.expect(campfire.matches(PropertyBlockData.of(campfire.getAsString())), "default matches works");
    }
}
