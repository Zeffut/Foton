package org.bukkit;

/** Vanilla painting variants and their block dimensions. */
public enum Art implements Keyed {
 KEBAB(1,1), AZTEC(1,1), ALBAN(1,1), AZTEC2(1,1), BOMB(1,1), PLANT(1,1), WASTELAND(1,1), POOL(2,1), COURBET(2,1), SEA(2,1), SUNSET(2,1), CREEBET(2,1), WANDERER(1,2), GRAHAM(1,2), MATCH(2,2), BUST(2,2), STAGE(2,2), VOID(2,2), SKULL_AND_ROSES(2,2), WITHER(2,2), FIGHTERS(4,2), POINTER(4,4), PIGSCENE(4,4), BURNING_SKULL(4,4), SKELETON(4,3), DONKEY_KONG(4,3), EARTH(2,2), WIND(2,2), WATER(2,2), FIRE(2,2), BAROQUE(2,2), HUMBLE(2,2), MEDITATIVE(1,1), PRAIRIE_RIDE(1,2), UNPACKED(4,4), BACKYARD(3,4), BOUQUET(3,3), CAVEBIRD(3,3), CHANGING(4,2), COTAN(3,3), ENDBOSS(3,3), FERN(3,3), FINDING(4,2), LOWMIST(4,2), ORB(4,4), OWLEMONS(3,3), PASSAGE(4,2), POND(3,4), SUNFLOWERS(3,3), TIDES(3,3), DENNIS(3,3);
 private final int width,height; Art(int width,int height){this.width=width;this.height=height;}
 @Override public NamespacedKey getKey(){return NamespacedKey.minecraft(name().toLowerCase(java.util.Locale.ROOT));}
 public int getBlockWidth(){return width;} public int getBlockHeight(){return height;} public int getId(){return ordinal();}
 public static Art getByName(String name){if(name==null)return null; for(Art art:values())if(art.name().equalsIgnoreCase(name))return art; return null;}
}
