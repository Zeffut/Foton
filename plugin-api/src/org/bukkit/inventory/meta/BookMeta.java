package org.bukkit.inventory.meta;

import java.util.List;

/** Title, author and pages carried by a writable or written book.
 *
 * <p>Paper's BookMeta is also an Adventure {@code Book}. Adventure 5, which
 * Foton ships, seals {@code Book}, so this one only declares the same
 * methods with the same descriptors: {@code title(Component)} and
 * {@code author(Component)} return the meta, and {@code pages(List)} returns
 * a {@code Book} snapshot of it after setting the pages.</p>
 */
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
        return new Spigot(book) {
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

    abstract class Spigot extends ItemMeta.Spigot {
        private final BookMeta book;
        protected Spigot() { this(null); }
        protected Spigot(BookMeta book) { super(book); this.book = book; }
        public abstract void addPage(net.md_5.bungee.api.chat.BaseComponent[]... pages);
        public List<String> getPages() { return book == null ? List.of() : book.getPages(); }
        public void setPages(List<String> pages) { if (book != null) book.setPages(pages); }
    }

    int getPageCount();

    net.kyori.adventure.text.Component title();

    /** Sets the title, as section-sign text, and returns this meta. */
    BookMeta title(net.kyori.adventure.text.Component title);

    net.kyori.adventure.text.Component author();

    BookMeta author(net.kyori.adventure.text.Component author);

    java.util.List<net.kyori.adventure.text.Component> pages();

    /** Sets the pages; answers the book as it now reads. */
    net.kyori.adventure.inventory.Book pages(java.util.List<net.kyori.adventure.text.Component> pages);

    default net.kyori.adventure.inventory.Book pages(net.kyori.adventure.text.Component... pages) {
        return pages(java.util.Arrays.asList(pages));
    }

    net.kyori.adventure.text.Component page(int page);

    void page(int page, net.kyori.adventure.text.Component data);

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
