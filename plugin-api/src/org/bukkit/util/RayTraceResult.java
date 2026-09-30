package org.bukkit.util;

import java.util.Objects;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.Entity;

/** Where a ray trace hit, and the block or entity it hit. */
public class RayTraceResult {
    private final Vector hitPosition;
    private final Block hitBlock;
    private final BlockFace hitBlockFace;
    private final Entity hitEntity;

    private RayTraceResult(Vector hitPosition, Block hitBlock, BlockFace hitBlockFace, Entity hitEntity) {
        if (hitPosition == null) throw new IllegalArgumentException("Hit position is null!");
        this.hitPosition = hitPosition.clone();
        this.hitBlock = hitBlock;
        this.hitBlockFace = hitBlockFace;
        this.hitEntity = hitEntity;
    }

    public RayTraceResult(Vector hitPosition) { this(hitPosition, null, null, null); }
    public RayTraceResult(Vector hitPosition, BlockFace hitBlockFace) { this(hitPosition, null, hitBlockFace, null); }
    public RayTraceResult(Vector hitPosition, Block hitBlock, BlockFace hitBlockFace) { this(hitPosition, hitBlock, hitBlockFace, null); }
    public RayTraceResult(Vector hitPosition, Entity hitEntity) { this(hitPosition, null, null, hitEntity); }
    public RayTraceResult(Vector hitPosition, Entity hitEntity, BlockFace hitBlockFace) { this(hitPosition, null, hitBlockFace, hitEntity); }

    public Vector getHitPosition() { return hitPosition.clone(); }
    public Block getHitBlock() { return hitBlock; }
    public BlockFace getHitBlockFace() { return hitBlockFace; }
    public Entity getHitEntity() { return hitEntity; }

    @Override public int hashCode() {
        final int prime = 31;
        int result = 1;
        result = prime * result + hitPosition.hashCode();
        result = prime * result + (hitBlock == null ? 0 : hitBlock.hashCode());
        result = prime * result + (hitBlockFace == null ? 0 : hitBlockFace.hashCode());
        result = prime * result + (hitEntity == null ? 0 : hitEntity.hashCode());
        return result;
    }

    @Override public boolean equals(Object obj) {
        if (this == obj) return true;
        if (!(obj instanceof RayTraceResult other)) return false;
        return hitPosition.equals(other.hitPosition) && Objects.equals(hitBlock, other.hitBlock)
            && Objects.equals(hitBlockFace, other.hitBlockFace) && Objects.equals(hitEntity, other.hitEntity);
    }

    @Override public String toString() {
        return "RayTraceResult [hitPosition=" + hitPosition + ", hitBlock=" + hitBlock
            + ", hitBlockFace=" + hitBlockFace + ", hitEntity=" + hitEntity + "]";
    }
}
