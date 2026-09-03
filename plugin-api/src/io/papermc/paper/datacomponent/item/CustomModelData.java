package io.papermc.paper.datacomponent.item;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Paper-style custom model data component. */
public final class CustomModelData {
    private final List<Float> floats; private final List<Boolean> flags; private final List<String> strings; private final List<Integer> colors;
    private CustomModelData(List<Float> f,List<Boolean> b,List<String> s,List<Integer> c){floats=List.copyOf(f);flags=List.copyOf(b);strings=List.copyOf(s);colors=List.copyOf(c);}
    public static Builder customModelData(){return new Builder();}
    public List<Float> floats(){return floats;} public List<Boolean> flags(){return flags;} public List<String> strings(){return strings;} public List<Integer> colors(){return colors;}
    public static final class Builder {
        private final List<Float> floats=new ArrayList<>(); private final List<Boolean> flags=new ArrayList<>(); private final List<String> strings=new ArrayList<>(); private final List<Integer> colors=new ArrayList<>();
        public Builder addFloat(float value){floats.add(value);return this;} public Builder addFlag(boolean value){flags.add(value);return this;} public Builder addString(String value){if(value!=null)strings.add(value);return this;} public Builder addColor(int value){colors.add(value);return this;}
        public Object build(){return new CustomModelData(floats,flags,strings,colors);}
    }
}
