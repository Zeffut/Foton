package org.bukkit.block.data;
import org.bukkit.block.BlockFace;
public final class SimpleRotatableData extends SimpleBlockData implements Rotatable {
 private static final BlockFace[] R={BlockFace.SOUTH,BlockFace.SOUTH_SOUTH_WEST,BlockFace.SOUTH_WEST,BlockFace.WEST_SOUTH_WEST,BlockFace.WEST,BlockFace.WEST_NORTH_WEST,BlockFace.NORTH_WEST,BlockFace.NORTH_NORTH_WEST,BlockFace.NORTH,BlockFace.NORTH_NORTH_EAST,BlockFace.NORTH_EAST,BlockFace.EAST_NORTH_EAST,BlockFace.EAST,BlockFace.EAST_SOUTH_EAST,BlockFace.SOUTH_EAST,BlockFace.SOUTH_SOUTH_EAST};
 public SimpleRotatableData(String text){super(text);}
 @Override public BlockFace getRotation(){int s=text.indexOf("rotation=");if(s<0)return R[0];int e=text.indexOf(',',s);if(e<0)e=text.indexOf(']',s);try{return R[Math.floorMod(Integer.parseInt(text.substring(s+9,e<0?text.length():e).trim()),16)];}catch(NumberFormatException x){return R[0];}}
 @Override public void setRotation(BlockFace f){if(f==null)return;for(int i=0;i<R.length;i++)if(R[i]==f){property("rotation",Integer.toString(i));return;}throw new IllegalArgumentException("Not a horizontal rotation: "+f);}
}
