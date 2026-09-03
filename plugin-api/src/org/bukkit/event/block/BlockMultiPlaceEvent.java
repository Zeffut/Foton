package org.bukkit.event.block;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.bukkit.block.Block;
import org.bukkit.block.BlockState;
import org.bukkit.entity.Player;
import org.bukkit.inventory.EquipmentSlot;
import org.bukkit.inventory.ItemStack;

/** Called when one placement action replaces multiple block states. */
public class BlockMultiPlaceEvent extends BlockPlaceEvent {
    private final List<BlockState> replacedStates;
    public BlockMultiPlaceEvent(List<BlockState> replacedStates, Block clicked, ItemStack itemInHand,
            Player thePlayer, boolean canBuild) {
        this(replacedStates, clicked, itemInHand, thePlayer, canBuild, null);
    }
    public BlockMultiPlaceEvent(List<BlockState> replacedStates, Block clicked, ItemStack itemInHand,
            Player thePlayer, boolean canBuild, EquipmentSlot hand) {
        super(clicked, clicked == null ? null : clicked.getState(), null, itemInHand, thePlayer, canBuild, hand);
        this.replacedStates = replacedStates == null ? List.of() :
            Collections.unmodifiableList(new ArrayList<>(replacedStates));
    }
    public List<BlockState> getReplacedBlockStates() { return replacedStates; }
}
