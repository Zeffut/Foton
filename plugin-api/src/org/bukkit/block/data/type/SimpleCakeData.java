package org.bukkit.block.data.type;
public final class SimpleCakeData extends org.bukkit.block.data.SimpleBlockData implements Cake {
 public SimpleCakeData(String text){super(text);} @Override public int getBites(){String v=propertyValue("bites");try{return Integer.parseInt(v);}catch(NumberFormatException e){return 0;}} @Override public void setBites(int v){if(v<0||v>6)throw new IllegalArgumentException("Cake bites must be between 0 and 6");property("bites",Integer.toString(v));} @Override public int getMaximumBites(){return 6;}
}
