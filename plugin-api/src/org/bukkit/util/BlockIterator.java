package org.bukkit.util;

import java.util.Iterator;
import java.util.NoSuchElementException;
import org.bukkit.Location;
import org.bukkit.World;
import org.bukkit.block.Block;
import org.bukkit.block.BlockFace;
import org.bukkit.entity.LivingEntity;

public class BlockIterator implements Iterator<Block> {
    private static final int GRID_SIZE = 1 << 24;
    private final World world; private final int maxDistance; private final Block[] blockQueue = new Block[3];
    private int currentBlock, currentDistance, maxDistanceInt, secondError, thirdError, secondStep, thirdStep;
    private BlockFace mainFace, secondFace, thirdFace; private boolean end;
    public BlockIterator(World world, Vector start, Vector direction, double yOffset, int maxDistance) {
        if (world==null||start==null||direction==null) throw new IllegalArgumentException("world, start and direction must not be null");
        if (direction.lengthSquared()==0.0) throw new IllegalArgumentException("direction must have at least one non-zero component");
        if (maxDistance<0) throw new IllegalArgumentException("maxDistance must not be negative");
        this.world=world; this.maxDistance=maxDistance; Vector p=start.clone(); p.setY(p.getY()+yOffset);
        Block s=world.getBlockAt(floor(p.getX()),floor(p.getY()),floor(p.getZ()));
        double md=0,sd=0,td=0,mp=0,sp=0,tp=0;
        if (xl(direction)>md) { mainFace=xf(direction);md=xl(direction);mp=xp(direction,p,s);secondFace=yf(direction);sd=yl(direction);sp=yp(direction,p,s);thirdFace=zf(direction);td=zl(direction);tp=zp(direction,p,s); }
        if (yl(direction)>md) { mainFace=yf(direction);md=yl(direction);mp=yp(direction,p,s);secondFace=zf(direction);sd=zl(direction);sp=zp(direction,p,s);thirdFace=xf(direction);td=xl(direction);tp=xp(direction,p,s); }
        if (zl(direction)>md) { mainFace=zf(direction);md=zl(direction);mp=zp(direction,p,s);secondFace=xf(direction);sd=xl(direction);sp=xp(direction,p,s);thirdFace=yf(direction);td=yl(direction);tp=yp(direction,p,s); }
        double d=mp/md; secondError=floor((sp-sd*d)*GRID_SIZE);secondStep=round(sd/md*GRID_SIZE);thirdError=floor((tp-td*d)*GRID_SIZE);thirdStep=round(td/md*GRID_SIZE);
        if(secondError+secondStep<=0)secondError=-secondStep+1;if(thirdError+thirdStep<=0)thirdError=-thirdStep+1;
        Block last=s.getRelative(mainFace.getOppositeFace());if(secondError<0){secondError+=GRID_SIZE;last=last.getRelative(secondFace.getOppositeFace());}if(thirdError<0){thirdError+=GRID_SIZE;last=last.getRelative(thirdFace.getOppositeFace());}
        secondError-=GRID_SIZE;thirdError-=GRID_SIZE;blockQueue[0]=last;currentBlock=-1;scan();boolean found=false;for(int i=currentBlock;i>=0;i--)if(eq(blockQueue[i],s)){currentBlock=i;found=true;break;}if(!found)throw new IllegalStateException("Start block missed in BlockIterator");
        maxDistanceInt=round(maxDistance/(Math.sqrt(md*md+sd*sd+td*td)/md));
    }
    public BlockIterator(Location l,double y,int m){this(l.getWorld(),l.toVector(),l.getDirection(),y,m);} public BlockIterator(Location l,double y){this(l,y,0);} public BlockIterator(Location l){this(l,0D);} public BlockIterator(LivingEntity e,int m){this(e.getLocation(),e.getEyeHeight(),m);} public BlockIterator(LivingEntity e){this(e,0);}
    @Override public boolean hasNext(){scan();return currentBlock!=-1;} @Override public Block next(){scan();if(currentBlock<0)throw new NoSuchElementException();return blockQueue[currentBlock--];} @Override public void remove(){throw new UnsupportedOperationException("[BlockIterator] doesn't support block removal");}
    private void scan(){if(currentBlock>=0||end)return;if(maxDistance!=0&&currentDistance>maxDistanceInt){end=true;return;}currentDistance++;secondError+=secondStep;thirdError+=thirdStep;if(secondError>0&&thirdError>0){blockQueue[2]=blockQueue[0].getRelative(mainFace);if((long)secondStep*thirdError<(long)thirdStep*secondError){blockQueue[1]=blockQueue[2].getRelative(secondFace);blockQueue[0]=blockQueue[1].getRelative(thirdFace);}else{blockQueue[1]=blockQueue[2].getRelative(thirdFace);blockQueue[0]=blockQueue[1].getRelative(secondFace);}thirdError-=GRID_SIZE;secondError-=GRID_SIZE;currentBlock=2;}else if(secondError>0){blockQueue[1]=blockQueue[0].getRelative(mainFace);blockQueue[0]=blockQueue[1].getRelative(secondFace);secondError-=GRID_SIZE;currentBlock=1;}else if(thirdError>0){blockQueue[1]=blockQueue[0].getRelative(mainFace);blockQueue[0]=blockQueue[1].getRelative(thirdFace);thirdError-=GRID_SIZE;currentBlock=1;}else{blockQueue[0]=blockQueue[0].getRelative(mainFace);currentBlock=0;}}
    private static boolean eq(Block a,Block b){return a.getX()==b.getX()&&a.getY()==b.getY()&&a.getZ()==b.getZ();}private static BlockFace xf(Vector d){return d.getX()>0?BlockFace.EAST:BlockFace.WEST;}private static BlockFace yf(Vector d){return d.getY()>0?BlockFace.UP:BlockFace.DOWN;}private static BlockFace zf(Vector d){return d.getZ()>0?BlockFace.SOUTH:BlockFace.NORTH;}private static double xl(Vector d){return Math.abs(d.getX());}private static double yl(Vector d){return Math.abs(d.getY());}private static double zl(Vector d){return Math.abs(d.getZ());}private static double pos(double d,double p,int b){return d>0?p-b:b+1-p;}private static double xp(Vector d,Vector p,Block b){return pos(d.getX(),p.getX(),b.getX());}private static double yp(Vector d,Vector p,Block b){return pos(d.getY(),p.getY(),b.getY());}private static double zp(Vector d,Vector p,Block b){return pos(d.getZ(),p.getZ(),b.getZ());}private static int floor(double d){return(int)Math.floor(d);}private static int round(double d){return(int)Math.round(d);}
}
