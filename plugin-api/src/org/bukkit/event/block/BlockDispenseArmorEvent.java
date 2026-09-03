package org.bukkit.event.block;
import org.bukkit.block.Block;
import org.bukkit.entity.LivingEntity;
import org.bukkit.inventory.ItemStack;
public class BlockDispenseArmorEvent extends BlockDispenseEvent {
 private final LivingEntity target;
 public BlockDispenseArmorEvent(Block block, ItemStack dispensed, LivingEntity target){super(block,dispensed);this.target=target;}
 public LivingEntity getTargetEntity(){return target;}
}
