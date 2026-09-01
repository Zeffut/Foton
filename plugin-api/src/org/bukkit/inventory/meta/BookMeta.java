package org.bukkit.inventory.meta;

import java.util.List;

/** Title, author and pages carried by a writable or written book. */
public interface BookMeta extends ItemMeta {
    boolean hasTitle();

    String getTitle();

    boolean setTitle(String title);

    boolean hasAuthor();

    String getAuthor();

    void setAuthor(String author);

    boolean hasPages();

    String getPage(int page);

    void setPage(int page, String data);

    List<String> getPages();

    void setPages(List<String> pages);

    void setPages(String... pages);

    void addPage(String... pages);

    /** Spigot's component-page adapter, backed by this book's ordinary pages. */
    default Spigot spigot() {
        BookMeta book = this;
        return new Spigot() {
            @Override
            public void addPage(net.md_5.bungee.api.chat.BaseComponent[]... pages) {
                if (pages == null) return;
                for (net.md_5.bungee.api.chat.BaseComponent[] page : pages) {
                    if (page == null) { book.addPage(""); continue; }
                    StringBuilder text = new StringBuilder();
                    for (net.md_5.bungee.api.chat.BaseComponent component : page)
                        if (component != null) text.append(component.toLegacyText());
                    book.addPage(text.toString());
                }
            }
        };
    }

    abstract class Spigot {
        public abstract void addPage(net.md_5.bungee.api.chat.BaseComponent[]... pages);
    }

    int getPageCount();

    Generation getGeneration();

    void setGeneration(Generation generation);

    @Override
    BookMeta clone();

    enum Generation {
        ORIGINAL,
        COPY_OF_ORIGINAL,
        COPY_OF_COPY,
        TATTERED
    }
}
