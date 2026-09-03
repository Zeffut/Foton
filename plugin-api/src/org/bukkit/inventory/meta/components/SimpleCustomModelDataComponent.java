package org.bukkit.inventory.meta.components;
import java.util.ArrayList; import java.util.List;
public final class SimpleCustomModelDataComponent implements CustomModelDataComponent {
 private List<Float> floats=new ArrayList<>(); private List<Boolean> flags=new ArrayList<>(); private List<String> strings=new ArrayList<>(); private List<org.bukkit.Color> colors=new ArrayList<>();
 private static <T> List<T> copy(List<T> v){return v==null?new ArrayList<>():new ArrayList<>(v);}
 @Override public List<Float> getFloats(){return new ArrayList<>(floats);} @Override public void setFloats(List<Float> v){floats=copy(v);}
 @Override public List<Boolean> getFlags(){return new ArrayList<>(flags);} @Override public void setFlags(List<Boolean> v){flags=copy(v);}
 @Override public List<String> getStrings(){return new ArrayList<>(strings);} @Override public void setStrings(List<String> v){strings=copy(v);}
 @Override public List<org.bukkit.Color> getColors(){return new ArrayList<>(colors);} @Override public void setColors(List<org.bukkit.Color> v){colors=copy(v);}
 @Override public SimpleCustomModelDataComponent clone(){SimpleCustomModelDataComponent c=new SimpleCustomModelDataComponent();c.floats=copy(floats);c.flags=copy(flags);c.strings=copy(strings);c.colors=copy(colors);return c;}
 @Override public boolean equals(Object other){if(!(other instanceof SimpleCustomModelDataComponent c))return false;return floats.equals(c.floats)&&flags.equals(c.flags)&&strings.equals(c.strings)&&colors.equals(c.colors);}
 @Override public int hashCode(){return java.util.Objects.hash(floats,flags,strings,colors);}
}
