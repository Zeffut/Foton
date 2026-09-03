package com.destroystokyo.paper;
import net.md_5.bungee.api.chat.BaseComponent;
public final class Title {
    private final BaseComponent[] title, subtitle;
    private final int fadeIn, stay, fadeOut;
    private Title(Builder b) { title=b.title; subtitle=b.subtitle; fadeIn=b.fadeIn; stay=b.stay; fadeOut=b.fadeOut; }
    public BaseComponent[] getTitle() { return title.clone(); }
    public BaseComponent[] getSubtitle() { return subtitle.clone(); }
    public int getFadeIn() { return fadeIn; }
    public int getStay() { return stay; }
    public int getFadeOut() { return fadeOut; }
    public static Builder builder() { return new Builder(); }
    public static final class Builder {
        private BaseComponent[] title=new BaseComponent[0], subtitle=new BaseComponent[0];
        private int fadeIn=10, stay=70, fadeOut=20;
        public Builder title(BaseComponent... v) { title=v==null?new BaseComponent[0]:v.clone(); return this; }
        public Builder subtitle(BaseComponent... v) { subtitle=v==null?new BaseComponent[0]:v.clone(); return this; }
        public Builder fadeIn(int v) { fadeIn=v; return this; }
        public Builder stay(int v) { stay=v; return this; }
        public Builder fadeOut(int v) { fadeOut=v; return this; }
        public Title build() { return new Title(this); }
    }
}
